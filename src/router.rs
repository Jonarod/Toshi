use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use http::{Request, Response};
use hyper::{Body, Error, Method, Server};
use hyper::service::{make_service_fn, service_fn};
use serde::Deserialize;

use crate::handlers::*;
use crate::handlers::summary::flush;
use crate::index::SharedCatalog;
use crate::utils::{not_found, parse_path, Paths};

#[derive(Deserialize, Debug, Default)]
pub struct QueryOptions {
    pub pretty: Option<bool>,
    pub include_sizes: Option<bool>,
}

impl QueryOptions {
    #[inline]
    pub fn include_sizes(&self) -> bool {
        self.include_sizes.unwrap_or(false)
    }

    #[inline]
    pub fn pretty(&self) -> bool {
        self.pretty.unwrap_or(false)
    }
}

pub async fn asdf(req: Request<Body>, catalog: SharedCatalog, watcher: Arc<AtomicBool>) -> Result<Response<Body>, Error> {
    let watcher = Arc::clone(&watcher);
    let search_handler = SearchHandler::new(Arc::clone(&catalog));
    let index_handler = IndexHandler::new(Arc::clone(&catalog));
    let bulk_handler = BulkHandler::new(Arc::clone(&catalog), watcher.clone());
    let (parts, body) = req.into_parts();
//        async move {
    let query_options: QueryOptions = parts
        .uri
        .query()
        .and_then(|q| serde_urlencoded::from_str(q).ok())
        .unwrap_or_default();

    let method = parts.method;
    let Paths(idx, action, add) = parse_path(parts.uri.path());

//                tracing::info!("REQ = {:?}", path);

    match (method, idx, action) {
        (m, Some(idx), Some(action)) if m == Method::PUT => match action.as_ref() {
            "_create" => index_handler.create_index(body, idx.to_string()).await,
            _ => not_found().await,
        },
//                (m, idx, action) if m == Method::GET => match action {
//                    "_summary" => summary(Arc::clone(&summary_cat), idx.to_string(), query_options).await,
//                    "_flush" => flush(Arc::clone(&summary_cat), idx.to_string()).await,
//                    _ => not_found().await,
//                },
//                (m, [idx, action]) if m == Method::POST => match *action {
//                    "_bulk" => bulk_handler.bulk_insert(body, idx.to_string()).await,
//                    _ => not_found().await,
//                },
//                (m, idx, _) if m == Method::POST => search_handler.doc_search(body, idx.to_string()).await,
//                (m, [idx]) if m == Method::PUT => index_handler.add_document(body, idx.to_string()).await,
//                (m, [idx]) if m == Method::DELETE => index_handler.delete_term(body, idx.to_string()).await,
//                (m, [idx]) if m == Method::GET => {
//                    if idx == &"favicon.ico" {
//                        not_found().await
//                    } else {
//                        search_handler.all_docs(idx.to_string()).await
//                    }
//                }
        (m, _, _) if m == Method::GET => root::root().await,
        _ => not_found().await,
    }
//        }
}

pub async fn router_with_catalog(addr: &SocketAddr, catalog: SharedCatalog, watcher: Arc<AtomicBool>) -> Result<(), Error> {
    let svcfn = service_fn(move |req| async { asdf(req, Arc::clone(&catalog), Arc::clone(&watcher)).await });

    let routes = make_service_fn(move |_| svcfn);

    Server::bind(&addr).serve(routes).await?;
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use http::StatusCode;

    use lazy_static::lazy_static;
    use toshi_test::{get_localhost, TestServer};

    use crate::index::tests::create_test_catalog;

    use super::*;

//    #[test]
//    pub fn test_create_router() {
//        let addr = get_localhost();
//        let response = TEST_SERVER
//            .get("http://localhost:8080")
//            .perform()
//            .unwrap();
//
//        assert_eq!(response.status(), StatusCode::OK);
//        let buf = String::from_utf8(response.into_body().concat2().wait().unwrap().to_vec()).unwrap();
//        assert_eq!(buf, "{\"name\":\"Toshi Search\",\"version\":\"0.1.1\"}");
//    }
}
