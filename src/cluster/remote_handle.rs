use std::hash::{Hash, Hasher};
use std::iter::Iterator;

use rand::prelude::*;
use tonic::{Request as TowerRequest, Response};
use tracing::*;

use async_trait::async_trait;
use toshi_proto::cluster_rpc::{DeleteRequest, DocumentRequest, ResultReply, SearchRequest};

use crate::cluster::rpc_server::RpcClient;
use crate::cluster::RPCError;
use crate::error::Error;
use crate::handle::{IndexHandle, IndexLocation, AsyncHandle};
use crate::handlers::index::{AddDocument, DeleteDoc, DocsAffected};
use crate::query::Search;
use crate::results::SearchResults;

/// A reference to an index stored somewhere else on the cluster, this operates via calling
/// the remote host and full filling the request via rpc, we need to figure out a better way
/// (tower-buffer) on how to keep these clients.
#[derive(Clone)]
pub struct RemoteIndex {
    name: String,
    remotes: Vec<RpcClient>,
}

impl PartialEq for RemoteIndex {
    fn eq(&self, other: &RemoteIndex) -> bool {
        self.name == *other.name
    }
}

impl Eq for RemoteIndex {}

impl Hash for RemoteIndex {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.name.as_bytes());
    }
}

impl RemoteIndex {
    pub fn new(name: String, remote: RpcClient) -> Self {
        RemoteIndex::with_clients(name, vec![remote])
    }

    pub fn with_clients(name: String, remotes: Vec<RpcClient>) -> Self {
        Self { name, remotes }
    }
}

#[async_trait]
impl AsyncHandle for RemoteIndex {

    async fn search_index(&self, search: Search) -> Result<SearchResults, Error> {
        let name = self.name.clone();
        let clients = self.remotes.clone();
        info!("REQ = {:?}", search);
        let mut results = SearchResults::new(vec![]);
        for mut client in clients {
            let bytes = match serde_json::to_vec(&search) {
                Ok(v) => v,
                Err(_) => Vec::new(),
            };
            let req = TowerRequest::new(SearchRequest {
                index: name.clone(),
                query: bytes,
            });
            let node_results = client.search_index(req).await?.into_inner();
            let search_results: SearchResults = serde_json::from_slice(&node_results.doc).unwrap();
            results = results + search_results;
//            results.push(search_results);
        }

        Ok(results)
    }

    async fn add_document(&self, add: AddDocument) -> Result<(), Error> {
        let name = self.name.clone();
        let clients = self.remotes.clone();
        info!("REQ = {:?}", add);
        let mut random = rand::rngs::SmallRng::from_entropy();
        let mut client = clients.into_iter().choose(&mut random).unwrap();
        let bytes = match serde_json::to_vec(&add) {
            Ok(v) => v,
            Err(_) => Vec::new(),
        };
        let req = TowerRequest::new(DocumentRequest {
            index: name,
            document: bytes,
        });
        let req: Result<Response<ResultReply>, RPCError> = client.place_document(req).await
            .map_err(|e| {
                info!("ERR = {:?}", e);
                e.into()
            });


//        info!("RESPONSE = {:?}", req?.into_inner());


        Ok(())
    }

   async fn delete_term(&self, delete: DeleteDoc) -> Result<DocsAffected, Error> {
        let name = self.name.clone();
        let clients = self.remotes.clone();

        let mut results = 0u64;

        for mut client in clients {
            let bytes = match serde_json::to_vec(&delete) {
                Ok(v) => v,
                Err(_) => Vec::new(),
            };
            let req = TowerRequest::new(DeleteRequest {
                index: name.clone(),
                terms: bytes,
            });
            let result: Result<Response<ResultReply>, RPCError> = client.delete_document(req).await
                .map_err(|e| {
                    info!("ERR = {:?}", e);
                    e.into()
                });
            results += result?.into_inner().code as u64;
        }

        Ok(DocsAffected { docs_affected: results })
    }
}
