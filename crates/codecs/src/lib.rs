//! The contract every backend is held to, and one adapter per backend.
//!
//! The contract is what a server's typed body needs: decode a `Bytes` body
//! into `T` (owned, or borrowing from the body), encode `T` to a fresh `Vec`,
//! into a caller's buffer, or into the writer a framework hands a codec, and
//! say whether a failure is the client's syntax (400) or a valid body of the
//! wrong shape (422).
//!
//! Two kinds of backend:
//!
//! - [`Backend`]: a serde data format, decoding and encoding any
//!   `T: Serialize + Deserialize`. serde_json (the `Json<T>` baseline), every
//!   serde CBOR candidate, and serde_cbor (the oracle, never measured).
//! - [`Native`]: a format with its own derive, decoding and encoding only the
//!   types that implement [`NativeType`]. prost (the `Protobuf<T>` baseline)
//!   and minicbor's own `Encode`/`Decode`.
//!
//! Any copy a backend needs to meet the contract happens inside the adapter,
//! so it is inside every measurement. No adapter adds a check its library
//! lacks (trailing bytes, depth): conformance judges the library as a server
//! would get it, and [`Backend::TRAILING`] records which ones check.

pub mod backend;
pub mod body;
pub mod failure;
pub mod native;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub use body::Body;
pub use failure::{Class, Failure};
pub use native::{Native, NativeType};

/// The wire format a backend reads and writes, which picks a workload's
/// input bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    /// JSON (RFC 8259), the `Json<T>` baseline.
    Json,
    /// CBOR (RFC 8949) through serde: structs as maps keyed by field name.
    Cbor,
    /// Protocol Buffers, the `Protobuf<T>` baseline.
    Protobuf,
    /// CBOR through minicbor's derive: structs as arrays indexed by `#[n]`.
    MinicborNative,
}

impl Format {
    /// Stable identifier.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Cbor => "cbor",
            Self::Protobuf => "protobuf",
            Self::MinicborNative => "minicbor-native",
        }
    }

    /// The media type a server reads and writes the format under.
    #[must_use]
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Json => "application/json",
            Self::Cbor | Self::MinicborNative => "application/cbor",
            Self::Protobuf => "application/protobuf",
        }
    }
}

/// Whether a decode refuses bytes after the first data item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Trailing {
    /// The library's entry point refuses them.
    Refused,
    /// The library decodes the first item and ignores the rest.
    Ignored,
}

/// A serde backend, as a server uses one.
pub trait Backend: 'static {
    /// Stable identifier used in result files.
    const NAME: &'static str;
    /// The crate providing it, as named in `Cargo.toml`.
    const CRATE: &'static str;
    /// The format it reads and writes.
    const FORMAT: Format;
    /// Whether it can encode at all.
    const ENCODES: bool;
    /// Whether it is a candidate for adoption rather than a reference point.
    const ELIGIBLE: bool;
    /// What its decode entry point does with bytes after the item, as read
    /// from its source (conformance checks it).
    const TRAILING: Trailing;

    /// Decode an owned `T` from a body the server now owns.
    fn decode<T: DeserializeOwned>(body: bytes::Bytes) -> Result<T, Failure>;

    /// Decode a `T` that may borrow from `body`.
    fn decode_borrowed<'a, T: Deserialize<'a>>(body: &'a mut Body) -> Result<T, Failure>;

    /// Encode into a new buffer.
    fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, Failure>;

    /// Encode into `out`, which is cleared first and may already have capacity.
    fn encode_into<T: Serialize>(value: &T, out: &mut Vec<u8>) -> Result<(), Failure>;

    /// Encode into any writer: what a framework's codec is handed (Kynos's
    /// `BodyCodec::encode`).
    fn to_writer<T: Serialize, W: std::io::Write>(value: &T, out: W) -> Result<(), Failure>;
}

/// Receives a serde backend type chosen at run time.
pub trait Visit {
    /// What the visit produces.
    type Output;
    /// Called with the backend named on the command line.
    fn visit<B: Backend>(self) -> Self::Output;
}

/// Receives a native backend type chosen at run time.
pub trait VisitNative {
    /// What the visit produces.
    type Output;
    /// Called with the backend named on the command line.
    fn visit<N: Native>(self) -> Self::Output;
}

/// Declares the backends: [`compiled`] lists them and [`dispatch`] resolves
/// one by name. One line per backend, gated on its crate's feature.
macro_rules! registry {
    (
        serde: [$($(#[$gate:meta])* $backend:path),* $(,)?],
        native: [$($(#[$ngate:meta])* $native:path),* $(,)?] $(,)?
    ) => {
        /// Every backend compiled into this build, baselines first.
        #[must_use]
        // One `push` per gated line is what lets a line compile out.
        #[allow(clippy::vec_init_then_push)]
        pub fn compiled() -> Vec<&'static str> {
            let mut names = Vec::new();
            $( $(#[$gate])* names.push(<$backend as Backend>::NAME); )*
            $( $(#[$ngate])* names.push(<$native as Native>::NAME); )*
            names
        }

        /// Every serde backend compiled into this build.
        #[must_use]
        #[allow(clippy::vec_init_then_push)]
        pub fn compiled_serde() -> Vec<&'static str> {
            let mut names = Vec::new();
            $( $(#[$gate])* names.push(<$backend as Backend>::NAME); )*
            names
        }

        /// Dispatch `visitor` to the serde backend called `name`, if this
        /// build has it.
        pub fn dispatch<V: Visit>(name: &str, visitor: V) -> Option<V::Output> {
            $( $(#[$gate])* if name == <$backend as Backend>::NAME { return Some(visitor.visit::<$backend>()); } )*
            None
        }

        /// Dispatch `visitor` to the native backend called `name`, if this
        /// build has it.
        pub fn dispatch_native<V: VisitNative>(name: &str, visitor: V) -> Option<V::Output> {
            $( $(#[$ngate])* if name == <$native as Native>::NAME { return Some(visitor.visit::<$native>()); } )*
            None
        }
    };
}

registry! {
    serde: [
        backend::serde_json::SerdeJson,
        backend::serde_cbor::SerdeCbor,
        #[cfg(feature = "ciborium")] backend::ciborium::Ciborium,
        #[cfg(feature = "minicbor-serde")] backend::minicbor_serde::MinicborSerde,
        #[cfg(feature = "cbor4ii")] backend::cbor4ii::Cbor4ii,
        #[cfg(feature = "cbor2")] backend::cbor2::Cbor2,
        #[cfg(feature = "c2pa_cbor")] backend::c2pa_cbor::C2paCbor,
    ],
    native: [
        native::prost::Prost,
        #[cfg(feature = "minicbor")] native::minicbor::Minicbor,
    ],
}

/// The serde CBOR candidates this build has, for the suites that run every
/// one (conformance, the worked codec's tests): not the baselines or the
/// oracle.
#[must_use]
pub fn cbor_candidates() -> Vec<&'static str> {
    struct Probe;
    impl Visit for Probe {
        type Output = bool;
        fn visit<B: Backend>(self) -> bool {
            B::FORMAT == Format::Cbor && B::ELIGIBLE
        }
    }
    compiled_serde()
        .into_iter()
        .filter(|name| dispatch(name, Probe).unwrap_or(false))
        .collect()
}
