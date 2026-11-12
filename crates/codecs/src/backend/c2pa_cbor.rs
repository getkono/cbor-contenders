//! c2pa_cbor 0.78.2.
//!
//! A maintained descendant of serde_cbor's API. `from_slice` reads through a
//! `Cursor` (so a `Cow` field decodes owned: its lifetime is not the
//! input's), caps allocation and depth, and refuses trailing data.
//!
//! # What its error can tell apart
//!
//! `Syntax(String)` carries every refusal the decoder makes itself, so the
//! adapter answers all of them 400. That is right for an ill-formed head,
//! the depth limit, the allocation cap, trailing data and an empty body, and
//! wrong for the well-formed bodies of the wrong shape it also refuses there
//! (`decoder.rs`): an enum that is neither text nor a one-entry map, a unit
//! variant given data or a data variant given none, and an `Option` of a
//! simple value or an indefinite string. Telling them apart would mean
//! reading the message, which the adapter does not do. `Serde(String)` is
//! every refusal through serde's `Error::custom`: 422. Its `deserialize_struct`
//! forwards to `deserialize_any`, so a struct is read from an array as from
//! a map, which no error reports.
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
        // The module docs name what `Syntax` carries; invalid UTF-8 has its
        // own variant: all 400.
        Error::InvalidUtf8 | Error::Syntax(_) | Error::TrailingData => Class::Syntax,
        // Every serde refusal arrives through `Error::custom`.
        Error::Serde(_) | Error::Encoding(_) => Class::Data,
    };
    Failure::new(class, error)
}

/// A decode's failure. Reading the `Cursor` over the body fails only at its
/// end (`UnexpectedEof`), so its `Io` is the input stopping inside an item.
#[cold]
fn decode_failure(error: &c2pa_cbor::Error) -> Failure {
    match error {
        c2pa_cbor::Error::Io(_) => Failure::new(Class::Eof, error),
        _ => failure(error),
    }
}

impl Backend for C2paCbor {
    const NAME: &'static str = "c2pa_cbor";
    const CRATE: &'static str = "c2pa_cbor";
    const FORMAT: Format = Format::Cbor;
    const ENCODES: bool = true;
    const ELIGIBLE: bool = true;
    const TRAILING: Trailing = Trailing::Refused;

    fn decode<T: DeserializeOwned>(body: Bytes) -> Result<T, Failure> {
        c2pa_cbor::from_slice(&body).map_err(|e| decode_failure(&e))
    }

    fn decode_borrowed<'a, T: Deserialize<'a>>(body: &'a mut Body) -> Result<T, Failure> {
        let body: &'a Body = body;
        c2pa_cbor::from_slice(body.as_slice()).map_err(|e| decode_failure(&e))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_truncated_body_is_eof() {
        for body in [&[0x19, 0x01][..], &[0x62, 0x61], &[0x82, 0x01]] {
            let failure = C2paCbor::decode::<serde_cbor::Value>(Bytes::copy_from_slice(body)).expect_err("truncated");
            assert_eq!(failure.class, Class::Eof, "{body:02x?}: {failure}");
        }
    }
}
