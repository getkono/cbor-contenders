//! c2pa_cbor 0.78.2.
//!
//! A maintained descendant of serde_cbor's API. `from_slice` reads through a
//! `Cursor` (so a `Cow` field decodes owned: its lifetime is not the
//! input's), caps allocation and depth, and refuses trailing data.

use bytes::Bytes;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{Backend, Body, Class, Failure, Format, Trailing};

/// `c2pa_cbor::{from_slice, to_vec, to_writer}`.
#[derive(Debug)]
pub struct C2paCbor;

#[cold]
fn failure(error: &c2pa_cbor::Error) -> Failure {
    use c2pa_cbor::Error;
    let class = match error {
        Error::Io(_) => Class::Io,
        Error::Eof => Class::Eof,
        // Its `Syntax` also carries the trailing-data and empty-input
        // refusals, and invalid UTF-8 has its own variant: all 400.
        Error::InvalidUtf8 | Error::Syntax(_) | Error::TrailingData => Class::Syntax,
        // Every serde refusal arrives through `Error::custom`.
        Error::Serde(_) | Error::Encoding(_) => Class::Data,
    };
    Failure::new(class, error)
}

impl Backend for C2paCbor {
    const NAME: &'static str = "c2pa_cbor";
    const CRATE: &'static str = "c2pa_cbor";
    const FORMAT: Format = Format::Cbor;
    const ENCODES: bool = true;
    const ELIGIBLE: bool = true;
    const TRAILING: Trailing = Trailing::Refused;

    fn decode<T: DeserializeOwned>(body: Bytes) -> Result<T, Failure> {
        c2pa_cbor::from_slice(&body).map_err(|e| failure(&e))
    }

    fn decode_borrowed<'a, T: Deserialize<'a>>(body: &'a mut Body) -> Result<T, Failure> {
        let body: &'a Body = body;
        c2pa_cbor::from_slice(body.as_slice()).map_err(|e| failure(&e))
    }

    fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, Failure> {
        c2pa_cbor::to_vec(value).map_err(|e| failure(&e))
    }

    fn encode_into<T: Serialize>(value: &T, out: &mut Vec<u8>) -> Result<(), Failure> {
        out.clear();
        c2pa_cbor::to_writer(out, value).map_err(|e| failure(&e))
    }

    fn to_writer<T: Serialize, W: std::io::Write>(value: &T, out: W) -> Result<(), Failure> {
        c2pa_cbor::to_writer(out, value).map_err(|e| failure(&e))
    }
}
