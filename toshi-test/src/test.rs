use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use failure::{Error, format_err};
use futures::{Future, TryFutureExt, TryStreamExt, FutureExt};
use http::HttpTryFrom;
use hyper::{Body, Method, Request, Response, Uri};
use hyper::client::{Client, connect::Connect};
use hyper::header::{HeaderValue, IntoHeaderName};
use tokio::runtime::Runtime;
use tokio::timer::Timeout;
use tracing::warn;

use crate::{CONTENT_TYPE, Result};

pub struct TestRequest<'a, C: Connect> {
    client: &'a TestClient<C>,
    request: Request<Body>,
}

impl<'a, C: Connect> Deref for TestRequest<'a, C> {
    type Target = Request<Body>;

    fn deref(&self) -> &Request<Body> {
        &self.request
    }
}

impl<'a, C: Connect> DerefMut for TestRequest<'a, C> {
    fn deref_mut(&mut self) -> &mut Request<Body> {
        &mut self.request
    }
}

impl<'a, C: Connect + 'static> TestRequest<'a, C> {
    pub(crate) fn new<U>(client: &'a TestClient<C>, method: Method, uri: U) -> Self
        where
            Uri: HttpTryFrom<U>,
    {
        TestRequest {
            client,
            request: Request::builder().method(method).uri(uri).body(Body::empty()).unwrap(),
        }
    }

    pub fn perform(self) -> Result<Response<Body>> {
        self.client.perform(self)
    }

    pub(crate) fn request(self) -> Request<Body> {
        self.request
    }

    pub fn with_header<N>(mut self, name: N, value: HeaderValue) -> Self
        where
            N: IntoHeaderName,
    {
        self.headers_mut().insert(name, value);
        self
    }
}

#[async_trait::async_trait]
pub(crate) trait BodyReader {
    async fn read_body(&mut self, response: Response<Body>) -> Result<Vec<u8>>;
}

pub trait Server: Clone {
    fn run_future<F, R, E>(&self, future: F) -> Result<R>
        where
            F: Send + Unpin + 'static + Future<Output=Result<R>>,
            R: Send + 'static,
            E: failure::Fail;

    fn request_expiry(&self) -> Instant;

    fn run_request<F, R, E>(&self, f: F) -> Result<R>
        where
            F: Send + Unpin + 'static + Future<Output=Result<R>>,
            R: Send + 'static,
            E: failure::Fail;
}

#[async_trait::async_trait]
impl<T: Server + Send> BodyReader for T {
    async fn read_body(&mut self, response: Response<Body>) -> Result<Vec<u8>> {
        response.into_body().try_concat().map_ok(|c| c.to_vec()).map_err(|e| Error::from(e)).await
    }
}

pub struct TestClient<C: Connect> {
    pub(crate) client: Client<C, Body>,
    pub(crate) rt: Arc<RwLock<Runtime>>,
}

impl<C: Connect + 'static> TestClient<C> {
    pub fn get<U>(&self, uri: U) -> TestRequest<C>
        where
            Uri: HttpTryFrom<U>,
    {
        self.build_request(Method::GET, uri)
    }

    pub fn post<B, U>(&self, uri: U, body: B) -> TestRequest<C>
        where
            B: Into<Body>,
            Uri: HttpTryFrom<U>,
    {
        self.build_request_with_body(Method::POST, uri, body)
    }

    pub fn put<B, U>(&self, uri: U, body: B) -> TestRequest<C>
        where
            B: Into<Body>,
            Uri: HttpTryFrom<U>,
    {
        self.build_request_with_body(Method::PUT, uri, body)
    }

    pub fn build_request<U>(&self, method: Method, uri: U) -> TestRequest<C>
        where
            Uri: HttpTryFrom<U>,
    {
        TestRequest::new(self, method, uri)
    }

    pub fn build_request_with_body<B, U>(&self, method: Method, uri: U, body: B) -> TestRequest<C>
        where
            B: Into<Body>,
            Uri: HttpTryFrom<U>,
    {
        let mut request = self.build_request(method, uri);
        {
            let headers = request.headers_mut();
            headers.insert("CONTENT_TYPE", HeaderValue::from_str(CONTENT_TYPE).unwrap());
        }

        *request.body_mut() = body.into();
        request
    }

    pub fn perform(&self, req: TestRequest<C>) -> Result<Response<Body>> {
        let req_future = self.client.request(req.request()).map_err(|e| {
            warn!("Error from test client request {:?}", e);
            format_err!("request failed: {:?}", e).compat()
        });

        self.rt
            .write()
            .expect("???")
            .block_on(req_future)
            .map_err(|e| failure::Error::from(e))
    }
}

pub struct TestResponse {
    response: Response<Body>,
}

impl Deref for TestResponse {
    type Target = Response<Body>;

    fn deref(&self) -> &Response<Body> {
        &self.response
    }
}

impl DerefMut for TestResponse {
    fn deref_mut(&mut self) -> &mut Response<Body> {
        &mut self.response
    }
}

impl fmt::Debug for TestResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "TestResponse")
    }
}

impl TestResponse {
    pub async fn read_utf8_body(self) -> Result<String> {
        let buf = self.response.into_body().try_concat().await?;
        let s = String::from_utf8(buf.to_vec())?;
        Ok(s)
    }
}
