//! cbor2 1.1.6 (`std`).
//!
//! Borrows from the input. Its `from_slice` decodes the first item and, as
//! its documentation says, does not report trailing data (`validate_slice`
//! would, at the cost of a second pass the adapter does not add).

use bytes::Bytes;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{Backend, Body, Class, Failure, Format, Trailing};

/// `cbor2::{from_slice, to_vec, to_writer}`.
#[derive(Debug)]
pub struct Cbor2;

#[cold]
fn decode_failure(error: &cbor2::de::Error) -> Failure {
    use cbor2::de::Error;
    match error {
        Error::Io(io) => Failure::new(Class::Eof, io),
        Error::Syntax(offset) => Failure::new(Class::Syntax, error).at(Some(*offset)),
        Error::Semantic(offset, _) => Failure::new(Class::Data, error).at(*offset),
        Error::RecursionLimitExceeded => Failure::new(Class::Syntax, error),
    }
}

fn encode_failure(error: &cbor2::ser::Error) -> Failure {
    match error {
        cbor2::ser::Error::Io(_) => Failure::new(Class::Io, error),
        cbor2::ser::Error::Value(_) => Failure::new(Class::Data, error),
    }
}

impl Backend for Cbor2 {
    const NAME: &'static str = "cbor2";
    const CRATE: &'static str = "cbor2";
    const FORMAT: Format = Format::Cbor;
    const ENCODES: bool = true;
    const ELIGIBLE: bool = true;
    const TRAILING: Trailing = Trailing::Ignored;

    fn decode<T: DeserializeOwned>(body: Bytes) -> Result<T, Failure> {
        cbor2::from_slice(&body).map_err(|e| decode_failure(&e))
    }

    fn decode_borrowed<'a, T: Deserialize<'a>>(body: &'a mut Body) -> Result<T, Failure> {
        let body: &'a Body = body;
        cbor2::from_slice(body.as_slice()).map_err(|e| decode_failure(&e))
    }

    fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, Failure> {
        cbor2::to_vec(value).map_err(|e| encode_failure(&e))
    }

    fn encode_into<T: Serialize>(value: &T, out: &mut Vec<u8>) -> Result<(), Failure> {
        out.clear();
        cbor2::to_writer(value, out).map_err(|e| encode_failure(&e))
    }

    fn to_writer<T: Serialize, W: std::io::Write>(value: &T, out: W) -> Result<(), Failure> {
        cbor2::to_writer(value, out).map_err(|e| encode_failure(&e))
    }
}
