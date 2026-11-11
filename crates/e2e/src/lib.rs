//! Whole HTTP requests through a Kynos server, to size the codec's share of
//! one.
//!
//! `cell` measures a codec in isolation; this crate asks what that is worth
//! once a request also pays for HTTP/1 parsing, Kynos's routing, body
//! collection, the content-type and limit checks, response framing and the
//! executor. Each entry is a Kynos service whose four routes carry their
//! bodies the way an application would:
//!
//! - `serde_json`: `Json<T>`, the in-tree JSON body.
//! - `prost`: `Protobuf<T>`, the in-tree protobuf body.
//! - each serde CBOR candidate: `Encoded<Cbor<B>, T>`, the worked outside
//!   codec (`kynos-cbor`), with its JSON-data-model scan, which is part of
//!   what an application deploying it pays.
//!
//! Every format is paired with its *floor*: the same server and routes with
//! the codec removed, reading the body as `Binary<M>` under the same media
//! type (so the 415, limit and coding checks are the same), checksumming an
//! echo's body as a codec entry checksums its payload, and answering with the
//! format's reference bytes, encoded once at start-up. Whatever the floor
//! costs, no codec can win back, so for any metric `m` the codec's share of a
//! request is `(m(entry) - m(floor)) / m(entry)`, and the most any swap can
//! gain end to end is bounded by it.
//!
//! Two binaries drive it: `e2e-server` serves over TCP for load generators
//! (throughput and latency), and `e2e-count` runs a fixed number of requests
//! over an in-memory pipe on one thread through `Server::serve_io`,
//! deterministic enough for callgrind.

pub mod body;
pub mod entry;
pub mod loopback;

use codecs::Format;

/// A benchmarked route: a method, a path and the workload it carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Route {
    /// `POST /echo`: decode an `Echo`, check it, encode it back.
    EchoPost,
    /// `GET /json/large`: encode a 705-item `Catalog`.
    JsonLargeGet,
    /// `POST /json/large`: decode a 705-item `Catalog`, answer 204.
    JsonLargePost,
    /// `GET /json/small`: encode one `Item`.
    JsonSmallGet,
}

impl Route {
    /// Every route, in report order.
    pub const ALL: &'static [Self] = &[
        Self::EchoPost,
        Self::JsonLargeGet,
        Self::JsonLargePost,
        Self::JsonSmallGet,
    ];

    /// Stable identifier used on the command line and in result files (the
    /// kynos-bench scenario names, whatever the format).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::EchoPost => "echo-post",
            Self::JsonLargeGet => "json-large-get",
            Self::JsonLargePost => "json-large-post",
            Self::JsonSmallGet => "json-small-get",
        }
    }

    /// The route called `name`, if any.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|route| route.name() == name)
    }

    /// The request method.
    #[must_use]
    pub fn method(self) -> http::Method {
        match self {
            Self::EchoPost | Self::JsonLargePost => http::Method::POST,
            Self::JsonLargeGet | Self::JsonSmallGet => http::Method::GET,
        }
    }

    /// The request path.
    #[must_use]
    pub const fn path(self) -> &'static str {
        match self {
            Self::EchoPost => "/echo",
            Self::JsonLargeGet | Self::JsonLargePost => "/json/large",
            Self::JsonSmallGet => "/json/small",
        }
    }

    /// The status a correct server answers a well-formed request with.
    #[must_use]
    pub fn status(self) -> http::StatusCode {
        match self {
            Self::JsonLargePost => http::StatusCode::NO_CONTENT,
            Self::EchoPost | Self::JsonLargeGet | Self::JsonSmallGet => http::StatusCode::OK,
        }
    }
}

/// The floor entry for `format`.
#[must_use]
pub fn floor(format: Format) -> String {
    format!("floor-{}", format.name())
}
