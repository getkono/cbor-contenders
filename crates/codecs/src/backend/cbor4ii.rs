//! cbor4ii 1.2.4 (`serde1`, `use_std`, `half-f16`).
//!
//! Borrows from the input, holds a depth limit of 256, and stops after the
//! first item; its deserializer is private, so the adapter cannot ask where
//! the item ended.

use std::convert::Infallible;

use bytes::Bytes;
use cbor4ii::core::error::DecodeError as Core;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{Backend, Body, Class, Failure, Format, Trailing};

/// `cbor4ii::serde::{from_slice, to_vec, to_writer}`.
#[derive(Debug)]
pub struct Cbor4ii;

#[cold]
fn decode_failure(error: &cbor4ii::serde::DecodeError<Infallible>) -> Failure {
    let class = match error {
        cbor4ii::serde::DecodeError::Core(core) => match core {
            Core::Eof { .. } | Core::LengthOverflow { .. } => Class::Eof,
            // A reserved additional-information value or a malformed
            // indefinite item; text that is not UTF-8 (an invalid item); the
            // depth limit.
            Core::Unsupported { .. } | Core::RequireUtf8 { .. } | Core::DepthOverflow { .. } => Class::Syntax,
            // A well-formed item of another major type than the target wants,
            // an integer that does not fit, a definite length required where
            // an indefinite one came, a slice required where none exists.
            _ => Class::Data,
        },
        cbor4ii::serde::DecodeError::Custom(_) => Class::Data,
    };
    Failure::new(class, error)
}

fn encode_failure<E: std::fmt::Debug>(error: &cbor4ii::serde::EncodeError<E>) -> Failure {
    match error {
        cbor4ii::serde::EncodeError::Core(_) => Failure::new(Class::Io, format!("{error:?}")),
        cbor4ii::serde::EncodeError::Custom(message) => Failure::new(Class::Data, message),
    }
}

impl Backend for Cbor4ii {
    const NAME: &'static str = "cbor4ii";
    const CRATE: &'static str = "cbor4ii";
    const FORMAT: Format = Format::Cbor;
    const ENCODES: bool = true;
    const ELIGIBLE: bool = true;
    // `from_slice` calls `Deserialize::deserialize` and returns.
    const TRAILING: Trailing = Trailing::Ignored;

    fn decode<T: DeserializeOwned>(body: Bytes) -> Result<T, Failure> {
        cbor4ii::serde::from_slice(&body).map_err(|e| decode_failure(&e))
    }

    fn decode_borrowed<'a, T: Deserialize<'a>>(body: &'a mut Body) -> Result<T, Failure> {
        let body: &'a Body = body;
        cbor4ii::serde::from_slice(body.as_slice()).map_err(|e| decode_failure(&e))
    }

    fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, Failure> {
        cbor4ii::serde::to_vec(Vec::new(), value).map_err(|e| encode_failure(&e))
    }

    fn encode_into<T: Serialize>(value: &T, out: &mut Vec<u8>) -> Result<(), Failure> {
        // `to_vec` appends to the buffer it is given and hands it back, so
        // the caller's capacity is reused.
        let mut buffer = std::mem::take(out);
        buffer.clear();
        *out = cbor4ii::serde::to_vec(buffer, value).map_err(|e| encode_failure(&e))?;
        Ok(())
    }

    fn to_writer<T: Serialize, W: std::io::Write>(value: &T, out: W) -> Result<(), Failure> {
        cbor4ii::serde::to_writer(out, value).map_err(|e| encode_failure(&e))
    }
}
