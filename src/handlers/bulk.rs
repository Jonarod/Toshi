use std::error::Error;
use std::future::Future;
use std::iter::Iterator;
use std::str::from_utf8;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use bytes::Bytes;
use crossbeam::channel::{Receiver, Sender, unbounded};
use http::{StatusCode, Response};
use hyper::{Body, Chunk};
use parking_lot::RwLock;
use tantivy::{Document, IndexWriter};
use tantivy::schema::Schema;
use tokio::prelude::*;
use tracing::*;

use crate::index::IndexCatalog;
use crate::utils::empty_with_code;

#[derive(Clone)]
pub struct BulkHandler {
    catalog: Arc<RwLock<IndexCatalog>>,
    watcher: Arc<AtomicBool>,
}

impl BulkHandler {
    pub fn new(catalog: Arc<RwLock<IndexCatalog>>, watcher: Arc<AtomicBool>) -> Self {
        BulkHandler { catalog, watcher }
    }

    async fn index_documents(index_writer: Arc<RwLock<IndexWriter>>, doc_receiver: Receiver<Document>, watcher: Arc<AtomicBool>) -> Result<(), Box<dyn Error>> {
        let w = index_writer.read();
        let w = w;
        let watcher = watcher;
        let start = Instant::now();
        for doc in doc_receiver {
            w.add_document(doc);
        }

        info!("Piping Documents took: {:?}", start.elapsed());
        info!("Unlocking watcher...");
        watcher.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn parsing_documents(schema: Schema, doc_sender: Sender<Document>, line_recv: Receiver<Bytes>) -> Result<(), Box<dyn Error>> {
        for line in line_recv {
            if !line.is_empty() {
                if let Ok(text) = from_utf8(&line) {
                    if let Ok(doc) = schema.parse_document(text) {
                        debug!("Sending doc: {:?}", &doc);
                        doc_sender.send(doc)?
                    }
                }
            }
        }
        Ok(())
    }

    fn process_body(mut buf: Bytes, line: Chunk, sender: Sender<Bytes>) -> impl Future<Output=Bytes> {
        async move {
            buf.extend(line);
            let mut split = buf.split(|b| *b == b'\n').peekable();
            while let Some(l) = split.next() {
                if split.peek().is_none() {
                    return Bytes::from(l);
                }
                debug!("Bytes in buf: {}", buf.len());
                sender.send(Bytes::from(l)).expect("Line sender failed.");
            }
            buf
        }
    }

    pub async fn bulk_insert(&self, body: Body, index: String) -> Result<Response<Body>, hyper::Error> {
        self.watcher.store(true, Ordering::SeqCst);
        let index_lock = self.catalog.read();
        let index_handle = index_lock.get_index(&index).unwrap();
        let index = index_handle.get_index();
        let schema = index.schema();
        let (line_sender, line_recv) = index_lock.settings.get_channel::<Bytes>();
        let (doc_sender, doc_recv) = unbounded::<Document>();
        let writer = index_handle.get_writer();
        let num_threads = index_lock.settings.json_parsing_threads;
        let watcher_clone = Arc::clone(&self.watcher);
        let line_sender_clone = line_sender.clone();

        for _ in 0..num_threads {
            let schema = schema.clone();
            let doc_sender = doc_sender.clone();
            let line_recv = line_recv.clone();
            tokio::spawn( async move {
                BulkHandler::parsing_documents(schema.clone(), doc_sender.clone(), line_recv.clone()).await;
            });
        }

        let left = body
            .fold(Bytes::new(), move |buf, line| {
                let line_sender_clone = line_sender_clone.clone();
                BulkHandler::process_body(buf, line.unwrap_or_default(), line_sender_clone)
            }).await;


        if !left.is_empty() {
            line_sender.send(left).expect("Line sender failed #2");
        }
        tokio::spawn( async {
            BulkHandler::index_documents(writer, doc_recv, watcher_clone).await;
        });

        Ok(empty_with_code(StatusCode::CREATED))
    }
}

#[cfg(test)]
mod tests {
    use std::thread::sleep;
    use std::time::Duration;

    use tokio::runtime::Runtime;

    use crate::error::Error;
    use crate::handlers::SearchHandler;
    use crate::handlers::summary::flush;
    use crate::index::tests::*;
    use crate::results::SearchResults;

    use super::*;

//    #[test]
//    fn test_bulk_index() -> Result<(), Error> {
//        let mut runtime = Runtime::new()?;
//        let server = create_test_catalog("test_index");
//        let lock = Arc::new(AtomicBool::new(false));
//        let handler = BulkHandler::new(Arc::clone(&server), Arc::clone(&lock));
//
//        let body = r#"
//        {"test_text": "asdf1234", "test_i64": 123, "test_u64": 321, "test_unindex": "asdf"}
//        {"test_text": "asdf5678", "test_i64": 456, "test_u64": 678, "test_unindex": "asdf"}
//        {"test_text": "asdf9012", "test_i64": -12, "test_u64": 901, "test_unindex": "asdf"}"#;
//
//        let index_docs = handler.bulk_insert(Body::from(body), "test_index".into());
//        let result = runtime.block_on(index_docs);
//
//        let flush = flush(Arc::clone(&server), "test_index".to_string());
//        runtime.block_on(flush)?;
//        assert_eq!(result.is_ok(), true);
//        sleep(Duration::from_secs(2));
//
//        let search = SearchHandler::new(Arc::clone(&server));
//        let check_docs = runtime.block_on(search.all_docs("test_index".into()))?;
//        let body = runtime.block_on(check_docs.into_body().concat2())?;
//        let docs: SearchResults = serde_json::from_slice(&body.into_bytes())?;
//        Ok(assert_eq!(docs.hits, 8))
//    }
}
