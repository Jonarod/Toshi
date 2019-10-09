use http::header::CONTENT_TYPE;
use hyper::{Body, Response};

use crate::handlers::ResponseFuture;

const TOSHI_INFO: &[u8] = b"{\"name\":\"Toshi Search\",\"version\":\"0.1.1\"}";

pub async fn root() -> ResponseFuture {
    Ok(Response::builder()
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(TOSHI_INFO)).unwrap())
}

#[cfg(test)]
mod tests {

    use super::*;
    use bytes::Buf;
    use tokio::prelude::*;
    use tokio::runtime::Runtime;
    use futures::TryStreamExt;

    #[test]
    fn test_root() -> Result<(), hyper::Error> {
        let rt = Runtime::new().unwrap();
        let body = rt.block_on(root())
            .unwrap()
            .into_body()
            .try_concat();

        rt.block_on(body).map(|v| assert_eq!(v.bytes(), TOSHI_INFO))
    }
}
