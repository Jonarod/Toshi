
use failure::Fail;
use serde::{Deserialize, Serialize};

pub use self::node::*;

pub type BoxError = Box<dyn ::std::error::Error + Send + Sync + 'static>;
pub type GrpcError = tonic::Status;

pub mod consul;
pub mod node;
pub mod remote_handle;
pub mod rpc_server;
pub mod shard;

#[derive(Debug, Fail, Serialize, Deserialize)]
pub enum ClusterError {
    #[fail(display = "Node has no ID")]
    MissingNodeID,
    #[fail(display = "Unable to determine cluster ID")]
    MissingClusterID,
    #[fail(display = "Unable to write node ID: {}", _0)]
    FailedWritingNodeID(String),
    #[fail(display = "Failed registering cluster: {}", _0)]
    FailedRegisteringCluster(String),
    #[fail(display = "Failed registering Node: {}", _0)]
    FailedRegisteringNode(String),
    #[fail(display = "Failed reading NodeID: {}", _0)]
    FailedReadingNodeID(String),
    #[fail(display = "Unable to retrieve disk metadata: {}", _0)]
    FailedGettingDirectoryMetadata(String),
    #[fail(display = "Unable to retrieve block device metadata: {}", _0)]
    FailedGettingBlockDeviceMetadata(String),
    #[fail(display = "Unable to find that directory: {}", _0)]
    NoMatchingDirectoryFound(String),
    #[fail(display = "Unable to find that block device: {}", _0)]
    NoMatchingBlockDeviceFound(String),
    #[fail(display = "Unable to read device RAM information: {}", _0)]
    FailedGettingRAMMetadata(String),
    #[fail(display = "Unable to get CPU metadata: {}", _0)]
    FailedGettingCPUMetadata(String),
    #[fail(display = "Unable to read content as UTF-8")]
    UnableToReadUTF8,
    #[fail(display = "Unable to create PrimaryShard: {}", _0)]
    FailedCreatingPrimaryShard(String),
    #[fail(display = "Unable to get index: {}", _0)]
    FailedGettingIndex(String),
    #[fail(display = "Unable to create ReplicaShard: {}", _0)]
    FailedCreatingReplicaShard(String),
    #[fail(display = "Failed to fetch nodes: {}", _0)]
    FailedFetchingNodes(String),
    #[fail(display = "Unable to get index name: {}", _0)]
    UnableToGetIndexName(String),
    #[fail(display = "Error parsing response from Consul: {}", _0)]
    ErrorParsingConsulJSON(String),
    #[fail(display = "Request from Consul returned an error: {}", _0)]
    ErrorInConsulResponse(String),
    #[fail(display = "Unable to get index handle")]
    UnableToGetIndexHandle,
    #[fail(display = "Unable to store services")]
    UnableToStoreServices,
}

#[derive(Debug, Fail)]
pub enum RPCError {
    #[fail(display = "Error in RPC: {}", _0)]
    RPCError(tonic::Status),
    #[fail(display = "")]
    BoxError(Box<dyn ::std::error::Error + Send + Sync + 'static>),
}



impl From<BoxError> for RPCError {
    fn from(err: BoxError) -> Self {
        RPCError::BoxError(err)
    }
}

impl From<tonic::Status> for RPCError {
    fn from(err: tonic::Status) -> Self {
        RPCError::RPCError(err)
    }
}
