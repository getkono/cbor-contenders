//! minicbor-serde 0.7.2 over minicbor 2.3.1 (`std`, `half`).
//!
//! Borrows `&str` and `&[u8]` from the input, and stops after the first item.
//! Its `DecodeError` hides minicbor's error kind (the field is private and
//! has no accessor), so the adapter cannot read the class off the error.
//! Instead, on the failure path only, it asks minicbor's own decoder whether
//! the input is one well-formed item (`Decoder::skip`): if not, the client
//! sent bytes that are not CBOR (400); if so, the item has the wrong shape
//! (422). That second pass costs nothing on a body that decodes. `skip` does
//! not validate UTF-8 in text strings, so an invalid text string is classed
//! 422 where it should be 400; conformance shows where.

use bytes::Bytes;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{Backend, Body, Class, Failure, Format, Trailing};

/// `minicbor_serde::{from_slice, to_vec, Serializer}`.
#[derive(Debug)]
pub struct MinicborSerde;

#[cold]
fn decode_failure(input: &[u8], error: &minicbor_serde::error::DecodeError) -> Failure {
    let mut probe = minicbor::Decoder::new(input);
    match probe.skip() {
        Err(wellformed) if wellformed.is_end_of_input() => Failure::new(Class::Eof, error),
        Err(wellformed) => Failure::new(Class::Syntax, error).at(wellformed.position()),
        Ok(()) => Failure::new(Class::Data, error),
    }
}

fn encode_failure<E: std::fmt::Display>(error: &minicbor_serde::error::EncodeError<E>) -> Failure {
    match error.as_write() {
        Some(io) => Failure::new(Class::Io, io),
        None => Failure::new(Class::Data, error),
    }
}

impl Backend for MinicborSerde {
    const NAME: &'static str = "minicbor-serde";
    const CRATE: &'static str = "minicbor-serde";
    const FORMAT: Format = Format::Cbor;
    const ENCODES: bool = true;
    const ELIGIBLE: bool = true;
    // `from_slice` calls `T::deserialize` and returns.
    const TRAILING: Trailing = Trailing::Ignored;

    fn decode<T: DeserializeOwned>(body: Bytes) -> Result<T, Failure> {
        minicbor_serde::from_slice(&body).map_err(|e| decode_failure(&body, &e))
    }

    fn decode_borrowed<'a, T: Deserialize<'a>>(body: &'a mut Body) -> Result<T, Failure> {
        let body: &'a Body = body;
        let input = body.as_slice();
        minicbor_serde::from_slice(input).map_err(|e| decode_failure(input, &e))
    }

    fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, Failure> {
        minicbor_serde::to_vec(value).map_err(|e| encode_failure(&e))
    }

    fn encode_into<T: Serialize>(value: &T, out: &mut Vec<u8>) -> Result<(), Failure> {
        out.clear();
        value
            .serialize(&mut minicbor_serde::Serializer::new(out))
            .map_err(|e| encode_failure(&e))
    }

    fn to_writer<T: Serialize, W: std::io::Write>(value: &T, out: W) -> Result<(), Failure> {
        value
            .serialize(&mut minicbor_serde::Serializer::new(
                minicbor::encode::write::Writer::new(out),
            ))
            .map_err(|e| encode_failure(&e))
    }
}
