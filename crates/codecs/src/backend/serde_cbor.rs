//! The oracle: serde_cbor 0.11.2.
//!
//! Unmaintained (RUSTSEC-2021-0127, repository archived), so never an entry.
//! It is here because it shares no code with any candidate: conformance
//! compares every candidate's answers with its own, every encoder's output is
//! read back through it, and every workload's CBOR input is its `to_vec`
//! (preferred serialization: shortest heads, the shortest float that holds
//! the value exactly, definite lengths, structs as maps keyed by field name).

use bytes::Bytes;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{Backend, Body, Class, Failure, Format, Trailing};

/// `serde_cbor::{from_slice, to_vec, to_writer}`.
#[derive(Debug)]
pub struct SerdeCbor;

fn failure(error: &serde_cbor::Error) -> Failure {
    use serde_cbor::error::Category;
    let class = match error.classify() {
        Category::Io => Class::Io,
        Category::Syntax => Class::Syntax,
        Category::Data => Class::Data,
        Category::Eof => Class::Eof,
    };
    Failure::new(class, error).at(usize::try_from(error.offset()).ok())
}

impl Backend for SerdeCbor {
    const NAME: &'static str = "serde_cbor";
    const CRATE: &'static str = "serde_cbor";
    const FORMAT: Format = Format::Cbor;
    const ENCODES: bool = true;
    const ELIGIBLE: bool = false;
    // `from_slice` calls `Deserializer::end`.
    const TRAILING: Trailing = Trailing::Refused;

    fn decode<T: DeserializeOwned>(body: Bytes) -> Result<T, Failure> {
        serde_cbor::from_slice(&body).map_err(|e| failure(&e))
    }

    fn decode_borrowed<'a, T: Deserialize<'a>>(body: &'a mut Body) -> Result<T, Failure> {
        let body: &'a Body = body;
        serde_cbor::from_slice(body.as_slice()).map_err(|e| failure(&e))
    }

    fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, Failure> {
        serde_cbor::to_vec(value).map_err(|e| failure(&e))
    }

    fn encode_into<T: Serialize>(value: &T, out: &mut Vec<u8>) -> Result<(), Failure> {
        out.clear();
        serde_cbor::to_writer(out, value).map_err(|e| failure(&e))
    }

    fn to_writer<T: Serialize, W: std::io::Write>(value: &T, out: W) -> Result<(), Failure> {
        serde_cbor::to_writer(out, value).map_err(|e| failure(&e))
    }
}
