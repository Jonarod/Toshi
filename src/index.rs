use std::clone::Clone;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use async_stream::stream;
use hashbrown::HashMap;
use http::Uri;
use http::uri::Scheme;
use parking_lot::{Mutex, RwLock};
use tantivy::directory::MmapDirectory;
use tantivy::Index;
use tantivy::schema::Schema;
use tokio::prelude::*;

use toshi_proto::cluster_rpc::*;

use crate::cluster::remote_handle::RemoteIndex;
use crate::cluster::rpc_server::{RpcClient, RpcServer};
use crate::error::Error;
use crate::handle::{AsyncHandle, IndexHandle, LocalIndex};
use crate::handlers::index::{AddDocument, DeleteDoc, DocsAffected};
use crate::query::Search;
use crate::Result;
use crate::results::*;
use crate::settings::Settings;

pub type SharedCatalog = Arc<RwLock<IndexCatalog>>;

pub struct IndexCatalog {
    pub settings: Settings,
    base_path: PathBuf,
    local_handles: HashMap<String, LocalIndex>,
    remote_handles: Arc<Mutex<HashMap<String, RemoteIndex>>>,
}

impl IndexCatalog {
    pub fn with_path(base_path: PathBuf) -> Result<Self> {
        IndexCatalog::new(base_path, Settings::default())
    }

    pub fn new(base_path: PathBuf, settings: Settings) -> Result<Self> {
        let remote_idxs = Arc::new(Mutex::new(HashMap::new()));
        let local_idxs = HashMap::new();

        let mut index_cat = IndexCatalog {
            settings,
            base_path,
            local_handles: local_idxs,
            remote_handles: remote_idxs,
        };
        index_cat.refresh_catalog()?;

        Ok(index_cat)
    }

    pub async fn update_remote_indexes(&self) -> Result<()> {
        let cat = Arc::clone(&self.remote_handles);
        let indexes = IndexCatalog::refresh_multiple_nodes(self.settings.experimental_features.nodes.clone());

        futures::pin_mut!(indexes);
        while let Some((c, idx)) = indexes.next().await {
            for i in idx {
                let ri = RemoteIndex::new(i.clone(), c.clone());
                cat.lock().insert(i.clone(), ri);
            }
        }
        Ok(())
    }

    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }

    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn with_index(name: String, index: Index) -> Result<Self> {
        let mut map = HashMap::new();
        let remote_map = HashMap::new();
        let new_index = LocalIndex::new(index, Settings::default(), &name)
            .unwrap_or_else(|_| panic!("Unable to open index: {} because it's locked", name));
        map.insert(name, new_index);

        Ok(IndexCatalog {
            settings: Settings::default(),
            base_path: PathBuf::new(),
            local_handles: map,
            remote_handles: Arc::new(Mutex::new(remote_map)),
        })
    }

    pub fn create_from_managed(mut base_path: PathBuf, index_path: &str, schema: Schema) -> Result<Index> {
        base_path.push(index_path);
        if !base_path.exists() {
            fs::create_dir(&base_path).map_err(|e| Error::IOError(e.to_string()))?;
        }
        let dir = MmapDirectory::open(base_path).map_err(|e| Error::IOError(e.to_string()))?;
        Index::open_or_create(dir, schema).map_err(|e| Error::IOError(e.to_string()))
    }

    pub fn load_index(path: &str) -> Result<Index> {
        let p = PathBuf::from(path);
        if p.exists() {
            Index::open_in_dir(&p)
                .map_err(|_| Error::UnknownIndex(p.display().to_string()))
                .and_then(Ok)
        } else {
            Err(Error::UnknownIndex(path.to_string()))
        }
    }

    pub fn add_index(&mut self, name: String, index: Index) -> Result<()> {
        let handle = LocalIndex::new(index, self.settings.clone(), &name)?;
        self.local_handles.insert(name.clone(), handle);
        Ok(())
    }

    pub fn add_remote_index(&mut self, name: String, remote: RpcClient) -> Result<()> {
        let ri = RemoteIndex::new(name.clone(), remote);
        self.remote_handles.lock().entry(name).or_insert(ri);
        Ok(())
    }

    pub fn add_multi_remote_index(&mut self, name: String, remote: Vec<RpcClient>) -> Result<()> {
        let ri = RemoteIndex::with_clients(name.clone(), remote);
        self.remote_handles.lock().entry(name).or_insert(ri);
        Ok(())
    }

    pub fn get_collection(&self) -> &HashMap<String, LocalIndex> {
        &self.local_handles
    }

    pub fn get_remote_collection(&self) -> Arc<Mutex<HashMap<String, RemoteIndex>>> {
        Arc::clone(&self.remote_handles)
    }

    pub fn get_mut_collection(&mut self) -> &mut HashMap<String, LocalIndex> {
        &mut self.local_handles
    }

    pub fn exists(&self, index: &str) -> bool {
        self.get_collection().contains_key(index)
    }

    pub fn remote_exists(&self, index: &str) -> bool {
        self.get_remote_collection().lock().contains_key(index)
    }

    pub fn get_mut_index(&mut self, name: &str) -> Result<&mut LocalIndex> {
        self.local_handles.get_mut(name).ok_or_else(|| Error::UnknownIndex(name.into()))
    }

    pub fn get_index(&self, name: &str) -> Result<&LocalIndex> {
        self.local_handles.get(name).ok_or_else(|| Error::UnknownIndex(name.into()))
    }

    pub fn get_owned_index(&self, name: &str) -> Result<LocalIndex> {
        self.local_handles
            .get(name)
            .cloned()
            .ok_or_else(|| Error::UnknownIndex(name.into()))
    }

    pub fn get_remote_index(&self, name: &str) -> Result<RemoteIndex> {
        self.get_remote_collection()
            .lock()
            .get(name)
            .cloned()
            .ok_or_else(|| Error::UnknownIndex(name.into()))
    }

    pub fn refresh_catalog(&mut self) -> Result<()> {
        self.local_handles.clear();

        for dir in fs::read_dir(self.base_path.clone())? {
            let entry = dir?.path();
            if let Some(entry_str) = entry.to_str() {
                if !entry_str.ends_with(".node_id") {
                    let pth: String = entry_str.rsplit('/').take(1).collect();
                    let idx = IndexCatalog::load_index(entry_str)?;
                    self.add_index(pth.clone(), idx)?;
                }
            } else {
                return Err(Error::IOError(format!("Path {} is not a valid unicode path", entry.display())));
            }
        }
        Ok(())
    }

    fn create_host_uri(socket: SocketAddr) -> Result<Uri> {
        Uri::builder()
            .scheme(Scheme::HTTP)
            .authority(socket.to_string().as_str())
            .path_and_query("")
            .build()
            .map_err(|e| Error::IOError(e.to_string()))
    }

    pub fn create_client(node: String) -> Result<RpcClient> {
        let socket: SocketAddr = node.parse().unwrap();
        let host_uri = IndexCatalog::create_host_uri(socket).unwrap();
        RpcServer::create_client(host_uri).map_err(Into::into)
    }

    pub fn refresh_multiple_nodes(nodes: Vec<String>) -> impl Stream<Item=(RpcClient, Vec<String>)> {
        stream! {
            for node in nodes.iter() {
               yield IndexCatalog::refresh_remote_catalog(node.to_owned()).await.unwrap();
            }
        }
    }

    pub async fn refresh_remote_catalog(node: String) -> Result<(RpcClient, Vec<String>)> {
        let mut client = IndexCatalog::create_client(node)?;
        client.list_indexes(tonic::Request::new(ListRequest {})).await
            .map(|resp| (client.clone(), resp.into_inner()))
            .map_err(Into::into)
            .map(move |(x, r)| (x, r.indexes))
    }

    pub async fn search_local_index(&self, index: &str, search: Search) -> Result<SearchResults> {
        if let Ok(idx) = self.get_index(index) {
            let s = idx.search_index(search)?;
            Ok(s)
        } else {
            Ok(SearchResults::new(vec![]))
        }
    }

    pub async fn search_remote_index(&self, index: &str, search: Search) -> Result<SearchResults> {
        let remote = self.get_remote_index(index)?;
        remote.search_index(search).await
    }

    pub async fn add_remote_document(&self, index: &str, doc: AddDocument) -> Result<()> {
        let hand = self.get_remote_index(index)?;
        hand.add_document(doc).await
    }

    pub fn add_local_document(&self, index: &str, doc: AddDocument) -> Result<()> {
        let hand = self.get_owned_index(index)?;
        hand.add_document(doc)
    }

    pub fn delete_local_term(&self, index: &str, term: DeleteDoc) -> Result<DocsAffected> {
        let hand = self.get_owned_index(index)?;
        hand.delete_term(term)
    }

    pub fn clear(&mut self) {
        self.local_handles.clear();
        self.remote_handles.lock().clear()
    }
}

#[cfg(test)]
pub mod tests {
    use std::sync::Arc;

    use parking_lot::RwLock;
    use tantivy::doc;
    use tantivy::schema::*;

    use super::*;

    pub fn create_test_catalog(name: &str) -> SharedCatalog {
        let idx = create_test_index();
        let catalog = IndexCatalog::with_index(name.into(), idx).unwrap();
        Arc::new(RwLock::new(catalog))
    }

    pub fn create_test_index() -> Index {
        let mut builder = SchemaBuilder::new();
        let test_text = builder.add_text_field("test_text", STORED | TEXT);
        let test_int = builder.add_i64_field("test_i64", STORED | INDEXED);
        let test_unsign = builder.add_u64_field("test_u64", STORED | INDEXED);
        let test_unindexed = builder.add_text_field("test_unindex", STORED);
        let test_facet = builder.add_facet_field("test_facet");

        let schema = builder.build();
        let idx = Index::create_in_ram(schema);
        let mut writer = idx.writer(30_000_000).unwrap();

        writer.add_document(doc! { test_text => "Test Document 1", test_int => 2014i64, test_unsign => 10u64, test_unindexed => "no", test_facet => Facet::from("/cat/cat2") });
        writer.add_document(doc! { test_text => "Test Dockument 2", test_int => - 2015i64, test_unsign => 11u64, test_unindexed => "yes", test_facet => Facet::from("/cat/cat2") });
        writer.add_document(doc! { test_text => "Test Duckiment 3", test_int => 2016i64, test_unsign => 12u64, test_unindexed => "noo", test_facet => Facet::from("/cat/cat3") });
        writer.add_document(doc! { test_text => "Test Document 4", test_int => - 2017i64, test_unsign => 13u64, test_unindexed => "yess", test_facet => Facet::from("/cat/cat4") });
        writer.add_document(doc! { test_text => "Test Document 5", test_int => 2018i64, test_unsign => 14u64, test_unindexed => "nooo", test_facet => Facet::from("/dog/cat2") });
        writer.commit().unwrap();

        idx
    }
}
