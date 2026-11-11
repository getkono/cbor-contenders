//! ciborium 0.2.2.
//!
//! Its entry points are `DeserializeOwned` only (it never hands out a slice of
//! the input), read through `std::io::Read`, and stop after the first item.

use bytes::Bytes;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{Backend, Body, Class, Failure, Format, Trailing};

/// `ciborium::{from_reader, into_writer}`.
#[derive(Debug)]
pub struct Ciborium;

fn decode_failure(error: &ciborium::de::Error<std::io::Error>) -> Failure {
    use ciborium::de::Error;
    match error {
        // Reading a slice fails only at its end: the input stopped inside an
        // item.
        Error::Io(io) => Failure::new(Class::Eof, io),
        Error::Syntax(offset) => Failure::new(Class::Syntax, "syntax error").at(Some(*offset)),
        // Every serde refusal (wrong type, unknown variant, integer out of
        // range, duplicate field) and ciborium's own checks on a well-formed
        // item. It cannot tell an invalid item (a bignum tag on a non-byte
        // string) from a wrong shape: both arrive here.
        Error::Semantic(offset, message) => Failure::new(Class::Data, message).at(*offset),
        Error::RecursionLimitExceeded => Failure::new(Class::Syntax, "recursion limit exceeded"),
    }
}

fn encode_failure(error: &ciborium::ser::Error<std::io::Error>) -> Failure {
    match error {
        ciborium::ser::Error::Io(io) => Failure::new(Class::Io, io),
        ciborium::ser::Error::Value(message) => Failure::new(Class::Data, message),
    }
}

impl Backend for Ciborium {
    const NAME: &'static str = "ciborium";
    const CRATE: &'static str = "ciborium";
    const FORMAT: Format = Format::Cbor;
    const ENCODES: bool = true;
    const ELIGIBLE: bool = true;
    // `from_reader` calls `T::deserialize` and returns.
    const TRAILING: Trailing = Trailing::Ignored;

    fn decode<T: DeserializeOwned>(body: Bytes) -> Result<T, Failure> {
        ciborium::from_reader(&body[..]).map_err(|e| decode_failure(&e))
    }

    fn decode_borrowed<'a, T: Deserialize<'a>>(_: &'a mut Body) -> Result<T, Failure> {
        // Every entry point requires `DeserializeOwned` and the
        // `Deserializer` is private, so a type that borrows from the body
        // cannot be decoded at all.
        Err(Failure::unsupported())
    }

    fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, Failure> {
        let mut out = Vec::new();
        ciborium::into_writer(value, &mut out).map_err(|e| encode_failure(&e))?;
        Ok(out)
    }

    fn encode_into<T: Serialize>(value: &T, out: &mut Vec<u8>) -> Result<(), Failure> {
        out.clear();
        ciborium::into_writer(value, out).map_err(|e| encode_failure(&e))
    }

    fn to_writer<T: Serialize, W: std::io::Write>(value: &T, out: W) -> Result<(), Failure> {
        ciborium::into_writer(value, out).map_err(|e| encode_failure(&e))
    }
}
