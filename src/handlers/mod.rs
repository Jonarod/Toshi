use http::Response;
use hyper::Body;

pub use self::{bulk::BulkHandler, index::IndexHandler, search::SearchHandler, summary::summary};

pub mod bulk;
pub mod index;
pub mod root;
pub mod search;
pub mod summary;

pub type ResponseFuture = Result<Response<Body>, hyper::Error>;
