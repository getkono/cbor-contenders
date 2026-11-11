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

pub mod body;

use std::marker::PhantomData;

use bytes::Bytes;
use codecs::{Backend, Failure, Native};

use crate::body::{Catalog, Echo};

/// The two body types through one codec, serde or native.
pub trait Codec {
    /// The format it reads and writes.
    const FORMAT: codecs::Format;
    /// Decode an `Echo`.
    fn decode_echo(input: Bytes) -> Result<Echo, Failure>;
    /// Decode a `Catalog`.
    fn decode_catalog(input: Bytes) -> Result<Catalog, Failure>;
    /// Encode an `Echo`.
    fn encode_echo(value: &Echo) -> Result<Vec<u8>, Failure>;
    /// Encode a `Catalog`.
    fn encode_catalog(value: &Catalog) -> Result<Vec<u8>, Failure>;
}

/// A serde backend as a [`Codec`].
#[derive(Debug)]
pub struct Serde<B>(PhantomData<B>);

impl<B: Backend> Codec for Serde<B> {
    const FORMAT: codecs::Format = B::FORMAT;
    fn decode_echo(input: Bytes) -> Result<Echo, Failure> {
        B::decode(input)
    }
    fn decode_catalog(input: Bytes) -> Result<Catalog, Failure> {
        B::decode(input)
    }
    fn encode_echo(value: &Echo) -> Result<Vec<u8>, Failure> {
        B::encode(value)
    }
    fn encode_catalog(value: &Catalog) -> Result<Vec<u8>, Failure> {
        B::encode(value)
    }
}

/// A native backend as a [`Codec`].
#[derive(Debug)]
pub struct Natively<N>(PhantomData<N>);

impl<N: Native> Codec for Natively<N> {
    const FORMAT: codecs::Format = N::FORMAT;
    fn decode_echo(input: Bytes) -> Result<Echo, Failure> {
        N::decode(input)
    }
    fn decode_catalog(input: Bytes) -> Result<Catalog, Failure> {
        N::decode(input)
    }
    fn encode_echo(value: &Echo) -> Result<Vec<u8>, Failure> {
        N::encode(value)
    }
    fn encode_catalog(value: &Catalog) -> Result<Vec<u8>, Failure> {
        N::encode(value)
    }
}

/// Declares [`Selected`]: the first codec in this list whose feature is on,
/// else serde_json. A measurement build enables one; a test build may enable
/// several, and then the first wins.
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
        /// The codec this build weighs.
        #[cfg(not(any($(feature = $seen),*)))]
        pub type Selected = Serde<codecs::backend::serde_json::SerdeJson>;
    };
}

select! {
    "prost" => Natively<codecs::native::prost::Prost>,
    "ciborium" => Serde<codecs::backend::ciborium::Ciborium>,
    "minicbor-serde" => Serde<codecs::backend::minicbor_serde::MinicborSerde>,
    "minicbor" => Natively<codecs::native::minicbor::Minicbor>,
    "cbor4ii" => Serde<codecs::backend::cbor4ii::Cbor4ii>,
    "cbor2" => Serde<codecs::backend::cbor2::Cbor2>,
    "c2pa_cbor" => Serde<codecs::backend::c2pa_cbor::C2paCbor>,
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
        Ok(Err(failure)) | Err(failure) => failure.status().to_string().into_bytes(),
    }
}
