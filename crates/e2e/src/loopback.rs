//! A hyper client wired to a Kynos server over an in-memory pipe.
//!
//! No sockets, no second thread: a `tokio::io::duplex` connects a
//! `hyper::client::conn::http1` connection to Kynos's `Server::serve_io`,
//! both spawned on the caller's runtime. On a current-thread runtime the whole
//! exchange is deterministic, which is what instruction counts need. The
//! client's own work (request framing, response parsing) is inside every
//! count, for the floor as for a backend, so it cancels in `backend - floor`
//! but does sit in the denominator of the codec's share.
//!
//! Request bodies go out in [`CHUNK`]-byte frames. `echo-post` (~1.1 KiB)
//! fits one, arrives in the server's first read and is collected as one
//! frame sharing hyper's read buffer; `json-large-post` (~64 KiB) spans
//! several, so `collect().to_bytes()` copies them into a fresh buffer — the
//! two arrivals a real server sees.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use codecs::Format;
use http::header::{ACCEPT, CONTENT_LENGTH, CONTENT_TYPE, HOST, HeaderValue};
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use hyper::body::{Frame, SizeHint};
use hyper::client::conn::http1::SendRequest;
use hyper_util::rt::TokioIo;
use kynos::router::service::Service;
use kynos::server::Server;
use payloads::Workload;
use payloads::kynos::{EchoPost, JsonLarge, JsonSmall};

use crate::Route;

/// The pipe's buffer, per direction.
pub const PIPE: usize = 64 * 1024;

/// The largest request body frame the client sends.
pub const CHUNK: usize = 16 * 1024;

/// A request body sent as a fixed sequence of frames.
#[derive(Debug)]
pub struct RequestBody {
    chunks: VecDeque<Bytes>,
    remaining: u64,
}

impl RequestBody {
    /// `bytes`, split into frames of at most [`CHUNK`] bytes. Slices share
    /// `bytes`; nothing is copied.
    #[must_use]
    pub fn chunked(bytes: &Bytes) -> Self {
        let chunks = (0..bytes.len())
            .step_by(CHUNK)
            .map(|start| bytes.slice(start..(start + CHUNK).min(bytes.len())))
            .collect();
        Self {
            chunks,
            remaining: bytes.len() as u64,
        }
    }
}

impl hyper::body::Body for RequestBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        let chunk = self.chunks.pop_front();
        if let Some(chunk) = &chunk {
            self.remaining -= chunk.len() as u64;
        }
        Poll::Ready(chunk.map(|chunk| Ok(Frame::data(chunk))))
    }

    fn is_end_stream(&self) -> bool {
        self.chunks.is_empty()
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.remaining)
    }
}

/// The request body for `route` in `format`; empty for `GET`.
#[must_use]
pub fn body_for(format: Format, route: Route) -> Bytes {
    match route {
        Route::EchoPost => Bytes::from(EchoPost::bytes(format)),
        Route::JsonLargePost => Bytes::from(JsonLarge::bytes(format)),
        Route::JsonLargeGet | Route::JsonSmallGet => Bytes::new(),
    }
}

/// A request for `route` carrying `body` in `format`, with `Host`, `Accept`
/// and, when there is a body, `Content-Type` and `Content-Length`.
#[must_use]
pub fn request(format: Format, route: Route, body: &Bytes) -> Request<RequestBody> {
    let mut request = Request::new(RequestBody::chunked(body));
    *request.method_mut() = route.method();
    *request.uri_mut() = http::Uri::from_static(route.path());
    let headers = request.headers_mut();
    headers.insert(HOST, HeaderValue::from_static("localhost"));
    headers.insert(ACCEPT, HeaderValue::from_static(format.media_type()));
    if !body.is_empty() {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static(format.media_type()));
        headers.insert(CONTENT_LENGTH, HeaderValue::from(body.len()));
    }
    request
}

/// One keep-alive client connection to an in-process server.
#[derive(Debug)]
pub struct Loopback {
    format: Format,
    sender: SendRequest<RequestBody>,
}

impl Loopback {
    /// Serve one connection of `service` and connect a client to it, on the
    /// current runtime.
    pub async fn connect(format: Format, service: Service<()>) -> Result<Self, String> {
        let (client, server) = tokio::io::duplex(PIPE);
        let serving = Server::new(service)
            .serve_io(server, std::net::SocketAddr::from(([127, 0, 0, 1], 1)))
            .map_err(|error| error.to_string())?;
        tokio::spawn(async move {
            // A server error surfaces to the client as a failed exchange.
            let _ = serving.await;
        });
        let (sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(client))
            .await
            .map_err(|error| error.to_string())?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        Ok(Self { format, sender })
    }

    /// Send `request` and collect the whole response.
    pub async fn send(&mut self, request: Request<RequestBody>) -> Result<(StatusCode, Bytes), hyper::Error> {
        self.sender.ready().await?;
        let response = self.sender.send_request(request).await?;
        let status = response.status();
        let body = response.into_body().collect().await?.to_bytes();
        Ok((status, body))
    }

    /// Send `route`'s request with `body` and check the status is
    /// [`Route::status`]; the response body is collected and dropped.
    pub async fn exchange(&mut self, route: Route, body: &Bytes) -> Result<Result<(), String>, String> {
        let (status, _) = self
            .send(request(self.format, route, body))
            .await
            .map_err(|error| error.to_string())?;
        Ok(if status == route.status() {
            Ok(())
        } else {
            Err(format!(
                "{}: status {status}, expected {}",
                route.name(),
                route.status()
            ))
        })
    }
}

/// The value a correct response to `route` holds, read back from `body` in
/// `format` and compared with the workload's own: byte-exact is enough, and
/// otherwise the body is decoded by the format's reference decoder.
fn same_value(format: Format, route: Route, body: &[u8]) -> Result<bool, String> {
    fn read<W: Workload>(format: Format, body: &[u8]) -> Result<bool, String>
    where
        W::Owned: PartialEq + prost::Message + Default,
    {
        let value = W::value();
        Ok(match format {
            Format::Json => serde_json::from_slice::<W::Owned>(body).map_err(|e| e.to_string())? == value,
            Format::Cbor => serde_cbor::from_slice::<W::Owned>(body).map_err(|e| e.to_string())? == value,
            Format::Protobuf => <W::Owned as prost::Message>::decode(body).map_err(|e| e.to_string())? == value,
            Format::MinicborNative => return Err("not served".into()),
        })
    }
    match route {
        Route::EchoPost => read::<EchoPost>(format, body),
        Route::JsonLargeGet => read::<JsonLarge>(format, body),
        Route::JsonSmallGet => read::<JsonSmall>(format, body),
        Route::JsonLargePost => Ok(body.is_empty()),
    }
}

/// Serve one request for `route` in-process through `entry` and check the
/// status and the body's value.
///
/// Must run inside a tokio runtime. The outer error is a failure to serve at
/// all; the inner one is the entry's wrong answer, a result.
pub async fn verify(entry: &str, route: Route) -> Result<Result<(), String>, String> {
    let format = crate::entry::format(entry).ok_or_else(|| format!("entry {entry} is not compiled into this build"))?;
    let service = crate::entry::service(entry)
        .ok_or_else(|| format!("entry {entry} is not compiled into this build"))?
        .map_err(|error| error.to_string())?;
    let mut loopback = Loopback::connect(format, service).await?;
    let (status, body) = loopback
        .send(request(format, route, &body_for(format, route)))
        .await
        .map_err(|error| error.to_string())?;
    if status != route.status() {
        return Ok(Err(format!(
            "{}: status {status}, expected {}: {}",
            route.name(),
            route.status(),
            String::from_utf8_lossy(&body)
        )));
    }
    Ok(match same_value(format, route, &body) {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!("{}: the body holds another value", route.name())),
        Err(error) => Err(format!("{}: the body does not read back: {error}", route.name())),
    })
}
