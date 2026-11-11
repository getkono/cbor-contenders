//! cbor4ii 1.2.4 (`serde1`, `use_std`, `half-f16`).
//!
//! Borrows from the input, holds a depth limit of 256, and stops after the
//! first item. Its `Deserializer` is public (`cbor4ii::serde::Deserializer`,
//! with `into_inner`), so a caller could ask where the item ended; its
//! `from_slice` does not, and the adapter adds no check the library lacks.
//!
//! # What its error can tell apart
//!
//! A refused head arrives as `Mismatch` or `Unsupported` carrying the byte
//! it `found` (`core/dec.rs`, `serde/de.rs`), and that byte alone decides:
//!
//! - additional information 28 to 30 under any major type, or 31 (an
//!   indefinite length) on an integer or a tag, is not CBOR (RFC 8949
//!   §3, §3.2.4): 400;
//! - `ff`, a break where an item belongs, is not CBOR either: an indefinite
//!   container's own break is consumed by `is_break` before any item is
//!   decoded, so a break that reaches a decode is a stray one: 400;
//! - any other head is a well-formed item of a type the target does not
//!   take, including a tag (it decodes only tags 2 and 3, as bignums) and an
//!   unassigned simple value: 422.
//!
//! What that cannot tell apart, so conformance records it:
//!
//! - an indefinite string's chunk of another type (`7f 01 ff`, `5f 61 00
//!   ff`) is `Mismatch` with the chunk's head, the same error as that head
//!   met where a string was asked for (`01` into `String`): 422;
//! - `f8 18` (a two-byte simple value below 32, ill-formed) is `found: f8`,
//!   the head every two-byte simple value has: 422;
//! - on the `u128`/`i128` paths `found` is a tag *number*, not a head, so
//!   a tag numbered 28 to 31 or 255 there is classed 400;
//! - `RequireLength { found: Indefinite }` is an indefinite chunk inside an
//!   indefinite string (`7f 7f ff ff`, ill-formed: 400), but it is also an
//!   indefinite array decoded into a tuple, which is well-formed: 400 too.

use std::convert::Infallible;

use bytes::Bytes;
use cbor4ii::core::error::{DecodeError as Core, Len};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{Backend, Body, Class, Failure, Format, Trailing};

/// `cbor4ii::serde::{from_slice, to_vec, to_writer}`.
#[derive(Debug)]
pub struct Cbor4ii;

#[cold]
fn decode_failure(error: &cbor4ii::serde::DecodeError<Infallible>) -> Failure {
    let class = match error {
        cbor4ii::serde::DecodeError::Core(core) => match core {
            Core::Mismatch { found, .. } | Core::Unsupported { found, .. } => head(*found),
            // The slice ended: before a head or inside an argument (`Eof`),
            // or inside a string that is borrowed (`RequireLength` with the
            // bytes left).
            Core::Eof { .. } | Core::RequireLength { found: Len::Small(_) | Len::Big, .. } => Class::Eof,
            // A chunk of an indefinite string that is itself indefinite
            // (module docs).
            Core::RequireLength { found: Len::Indefinite, .. }
            // Text that is not UTF-8 (an invalid item, RFC 8949 §5.3.1).
            | Core::RequireUtf8 { .. }
            // The depth limit.
            | Core::DepthOverflow { .. } => Class::Syntax,
            // A bignum longer than 16 bytes (`LengthOverflow`), an integer
            // that does not fit the target, and a slice required where none
            // exists: a valid item the target cannot hold.
            _ => Class::Data,
        },
        cbor4ii::serde::DecodeError::Custom(_) => Class::Data,
    };
    Failure::new(class, error)
}

/// The class of a head cbor4ii refused (module docs).
fn head(found: u8) -> Class {
    let (major, info) = (found >> 5, found & 0x1f);
    match (major, info) {
        (_, 28..=30) | (0 | 1 | 6 | 7, 31) => Class::Syntax,
        _ => Class::Data,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        let text: String = text.split_whitespace().collect();
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
            .collect()
    }

    fn class<T: DeserializeOwned + std::fmt::Debug>(bytes: &str) -> Class {
        Cbor4ii::decode::<T>(Bytes::from(hex(bytes))).expect_err(bytes).class
    }

    #[test]
    fn ill_formed_heads_are_400() {
        for bytes in [
            "1c",
            "1d",
            "1e",
            "3d",
            "5e",
            "7c",
            "9d",
            "be",
            "dc",
            "fc",
            "fd",
            "fe",
            "1f",
            "3f",
            "df 00",
            "ff",
            "82 01 ff",
            "7f 7f ff ff",
            "5f 5f ff ff",
        ] {
            let got = class::<serde_cbor::Value>(bytes);
            assert!(matches!(got, Class::Syntax), "{bytes}: {got:?}");
        }
        // Into a typed target, too.
        assert!(matches!(class::<Vec<u64>>("82 01 1c"), Class::Syntax));
        assert!(matches!(class::<String>("7f 7f ff ff"), Class::Syntax));
    }

    #[test]
    fn invalid_text_and_truncation_are_400() {
        assert!(matches!(class::<String>("61 ff"), Class::Syntax));
        assert!(matches!(class::<String>("62 61"), Class::Eof));
        assert!(matches!(class::<u64>("19 01"), Class::Eof));
        assert!(matches!(class::<serde_cbor::Value>(""), Class::Eof));
    }

    #[test]
    fn well_formed_items_of_another_shape_are_422() {
        // A tag, where text was asked for.
        assert!(matches!(class::<String>("c1 00"), Class::Data));
        assert!(matches!(class::<serde_cbor::Value>("c1 00"), Class::Data));
        // A wrong type, an unassigned simple value, a bignum over 16 bytes.
        assert!(matches!(class::<String>("01"), Class::Data));
        assert!(matches!(class::<u64>("61 61"), Class::Data));
        assert!(matches!(class::<serde_cbor::Value>("f0"), Class::Data));
        assert!(matches!(
            class::<u128>("c2 51 0100000000000000000000000000000000"),
            Class::Data
        ));
        // An indefinite array is well-formed; only its own break closes it.
        assert_eq!(
            Cbor4ii::decode::<Vec<u64>>(Bytes::from(hex("9f 01 ff"))).expect("decodes"),
            vec![1]
        );
    }

    #[test]
    fn what_the_error_cannot_tell_apart_is_422() {
        // An indefinite string's chunk of another type is the same error as
        // that head where a string was asked for; `f8 18` is the head of
        // every two-byte simple value (module docs).
        for (bytes, alone) in [("7f 01 ff", "01"), ("7f 41 00 ff", "41 00")] {
            assert!(matches!(class::<String>(bytes), Class::Data), "{bytes}");
            assert!(matches!(class::<String>(alone), Class::Data), "{alone}");
        }
        assert!(matches!(class::<serde_bytes::ByteBuf>("5f 61 00 ff"), Class::Data));
        assert!(matches!(class::<serde_cbor::Value>("f8 18"), Class::Data));
    }
}
