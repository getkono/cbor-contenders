//! The fixture programs `xtask size` links and weighs.
//!
//! `fixture-full` decodes and re-encodes the two body types a handler would
//! declare (`Echo`, `Catalog`); `fixture-decode` only decodes them; `floor`
//! does the same I/O with no codec at all. A codec's cost is its fixture's
//! `.text` minus the floor's.
//!
//! The fixture holds the codec and the body types, not a Kynos server: every
//! CBOR codec reaches Kynos through the same `Encoded<C, T>`, so the glue a
//! server adds per codec is the same for each and is not what is compared.
//!
//! Each codec is called through its library's own entry points, as its
//! `codecs` adapter calls them, but not through `codecs`: that crate always
//! links the baselines and the oracle, whose code and compile time a
//! candidate's fixture must not carry. A failure is answered with its status
//! and the library's own message, as a server's problem body would carry it.

pub mod body;

use bytes::Bytes;

use crate::body::{Catalog, Echo};

/// A refusal: the status a server answers with, and the library's message.
pub type Refusal = (u16, String);

/// A syntax refusal (400) carrying `error`'s message.
fn refused(error: impl std::fmt::Display) -> Refusal {
    (400, error.to_string())
}

/// An encode failure (500) carrying `error`'s message. prost's encode into
/// a `Vec` cannot fail.
#[cfg(any(
    feature = "serde_json",
    feature = "ciborium",
    feature = "minicbor-serde",
    feature = "minicbor",
    feature = "cbor4ii",
    feature = "cbor2",
    feature = "c2pa_cbor"
))]
fn failed(error: impl std::fmt::Display) -> Refusal {
    (500, error.to_string())
}

/// The two body types through one codec.
pub trait Codec {
    /// Decode an `Echo`.
    fn decode_echo(input: Bytes) -> Result<Echo, Refusal>;
    /// Decode a `Catalog`.
    fn decode_catalog(input: Bytes) -> Result<Catalog, Refusal>;
    /// Encode an `Echo`.
    fn encode_echo(value: &Echo) -> Result<Vec<u8>, Refusal>;
    /// Encode a `Catalog`.
    fn encode_catalog(value: &Catalog) -> Result<Vec<u8>, Refusal>;
}

/// Implements [`Codec`] for `$name` over a serde library's
/// `decode(&Bytes)` and `encode(&T)`.
macro_rules! serde_codec {
    ($(#[$meta:meta])* $feature:literal, $name:ident, |$input:ident| $decode:expr, |$value:ident| $encode:expr) => {
        $(#[$meta])*
        #[cfg(feature = $feature)]
        #[derive(Debug)]
        pub struct $name;

        #[cfg(feature = $feature)]
        impl Codec for $name {
            fn decode_echo($input: Bytes) -> Result<Echo, Refusal> {
                $decode.map_err(refused)
            }
            fn decode_catalog($input: Bytes) -> Result<Catalog, Refusal> {
                $decode.map_err(refused)
            }
            fn encode_echo($value: &Echo) -> Result<Vec<u8>, Refusal> {
                $encode.map_err(failed)
            }
            fn encode_catalog($value: &Catalog) -> Result<Vec<u8>, Refusal> {
                $encode.map_err(failed)
            }
        }
    };
}

serde_codec!(
    /// serde_json, as `Json<T>` uses it.
    "serde_json", SerdeJson, |input| serde_json::from_slice(&input), |value| serde_json::to_vec(value)
);
serde_codec!(
    /// ciborium.
    "ciborium",
    Ciborium,
    |input| ciborium::from_reader(&input[..]),
    |value| {
        let mut out = Vec::new();
        ciborium::into_writer(value, &mut out).map(|()| out)
    }
);
serde_codec!(
    /// minicbor-serde.
    "minicbor-serde",
    MinicborSerde,
    |input| minicbor_serde::from_slice(&input),
    |value| minicbor_serde::to_vec(value)
);
serde_codec!(
    /// cbor4ii's serde API.
    "cbor4ii",
    Cbor4ii,
    |input| cbor4ii::serde::from_slice(&input),
    |value| cbor4ii::serde::to_vec(Vec::new(), value)
);
serde_codec!(
    /// cbor2.
    "cbor2", Cbor2, |input| cbor2::from_slice(&input), |value| cbor2::to_vec(value)
);
serde_codec!(
    /// c2pa_cbor.
    "c2pa_cbor", C2paCbor, |input| c2pa_cbor::from_slice(&input), |value| c2pa_cbor::to_vec(value)
);

/// prost, as `Protobuf<T>` uses it: `decode` on the body's `Bytes`,
/// `encode_to_vec`.
#[cfg(feature = "prost")]
#[derive(Debug)]
pub struct Prost;

#[cfg(feature = "prost")]
impl Codec for Prost {
    fn decode_echo(input: Bytes) -> Result<Echo, Refusal> {
        prost::Message::decode(input).map_err(refused)
    }
    fn decode_catalog(input: Bytes) -> Result<Catalog, Refusal> {
        prost::Message::decode(input).map_err(refused)
    }
    fn encode_echo(value: &Echo) -> Result<Vec<u8>, Refusal> {
        Ok(prost::Message::encode_to_vec(value))
    }
    fn encode_catalog(value: &Catalog) -> Result<Vec<u8>, Refusal> {
        Ok(prost::Message::encode_to_vec(value))
    }
}

/// minicbor's own derive.
#[cfg(feature = "minicbor")]
#[derive(Debug)]
pub struct Minicbor;

#[cfg(feature = "minicbor")]
impl Codec for Minicbor {
    fn decode_echo(input: Bytes) -> Result<Echo, Refusal> {
        minicbor::decode(&input).map_err(refused)
    }
    fn decode_catalog(input: Bytes) -> Result<Catalog, Refusal> {
        minicbor::decode(&input).map_err(refused)
    }
    fn encode_echo(value: &Echo) -> Result<Vec<u8>, Refusal> {
        minicbor::to_vec(value).map_err(failed)
    }
    fn encode_catalog(value: &Catalog) -> Result<Vec<u8>, Refusal> {
        minicbor::to_vec(value).map_err(failed)
    }
}

/// Declares [`Selected`]: the first codec in this list whose feature is on.
/// A measurement build enables exactly one; a lint or test build may enable
/// several (the defaults are serde_json and prost), and then the first wins.
macro_rules! select {
    ($($feature:literal => $codec:ty),* $(,)?) => {
        select!(@ [] $($feature => $codec),*);
    };
    (@ [$($seen:literal)*] $feature:literal => $codec:ty $(, $rf:literal => $rc:ty)*) => {
        /// The codec this build weighs.
        #[cfg(all(feature = $feature, not(any($(feature = $seen),*))))]
        pub type Selected = $codec;
        select!(@ [$($seen)* $feature] $($rf => $rc),*);
    };
    (@ [$($seen:literal)*]) => {
        #[cfg(not(any($(feature = $seen),*)))]
        compile_error!("size-fixture weighs one codec: enable one of its features");
    };
}

select! {
    "ciborium" => Ciborium,
    "minicbor-serde" => MinicborSerde,
    "minicbor" => Minicbor,
    "cbor4ii" => Cbor4ii,
    "cbor2" => Cbor2,
    "c2pa_cbor" => C2paCbor,
    "serde_json" => SerdeJson,
    "prost" => Prost,
}

/// Read all of stdin.
#[must_use]
pub fn stdin() -> Bytes {
    let mut input = Vec::new();
    let _ = std::io::Read::read_to_end(&mut std::io::stdin(), &mut input);
    input.into()
}

/// Which body type the input is: decided at run time, so the optimizer cannot
/// assume either path away. An `Echo` is about a kibibyte in every format, a
/// `Catalog` tens of kibibytes.
#[must_use]
pub fn is_echo(input: &[u8]) -> bool {
    input.len() < 4096
}

/// Decode (and, when `encode`, re-encode) through [`Selected`].
#[must_use]
pub fn round_trip(input: Bytes, encode: bool) -> Vec<u8> {
    let out = if is_echo(&input) {
        Selected::decode_echo(input).map(|v| {
            if encode {
                Selected::encode_echo(&v)
            } else {
                Ok(vec![v.payload.len() as u8])
            }
        })
    } else {
        Selected::decode_catalog(input).map(|v| {
            if encode {
                Selected::encode_catalog(&v)
            } else {
                Ok(vec![v.items.len() as u8])
            }
        })
    };
    match out {
        Ok(Ok(bytes)) => bytes,
        Ok(Err((status, message))) | Err((status, message)) => format!("{status} {message}").into_bytes(),
    }
}
