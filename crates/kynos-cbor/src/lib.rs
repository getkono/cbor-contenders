//! A worked outside `BodyCodec` for `application/cbor`.
//!
//! Kynos does not ship CBOR (kynos #989, #998): an application that needs it
//! brings a codec through `Encoded<C, T>` (kynos #988). This crate is that
//! codec, written the way the trust model asks: Kynos keeps the 415, the body
//! limit, the content-coding refusal, the 400/422 split and the description;
//! the codec turns bytes into `T` and back, and refuses every value the
//! schema Kynos derives from `T` could not describe.
//!
//! ```ignore
//! use kynos::extract::body::codec::Encoded;
//! use kynos_cbor::Cbor;
//! use codecs::backend::ciborium::Ciborium;
//!
//! #[kynos::post("/items")]
//! async fn create(item: Encoded<Cbor<Ciborium>, Item>) -> Encoded<Cbor<Ciborium>, Item> {
//!     item
//! }
//! ```
//!
//! `examples/serve.rs` serves one, negotiated beside `Json<T>`; the tests in
//! `tests/` drive every rule through Kynos for every CBOR library.
//!
//! # What it answers
//!
//! | Body | Status | Where |
//! | --- | --- | --- |
//! | Another `Content-Type`, too large, under a content coding | 415, 413, 415 | Kynos, before the codec |
//! | Not a valid CBOR item (ill-formed, invalid UTF-8, bytes after it, too deep) | 400 | [`faithful::scan`] |
//! | A valid item the JSON data model has no place for (byte string, tag, `undefined`, NaN, non-text key, repeated key) | 422 | [`faithful::scan`] |
//! | A valid item of the wrong shape for `T` | 422 | the library |
//! | A failed bound `T`'s schema declares | 422 | Kynos, after the codec |
//! | A response value the data model has no place for | 500 | [`faithful::Faithful`] |
//!
//! The scan runs first, so a library's own leniency (trailing bytes, ignored
//! tags, no depth limit) never reaches a handler; conformance measures each
//! library bare, so what the scan is protecting against is on record.
//!
//! # The schema it is honest about
//!
//! Everything accepted is JSON's data model written in CBOR: text, numbers,
//! booleans, `null`, arrays, and maps keyed by text. That is exactly what the
//! schema Kynos derives from a `T: Schema` describes, so the description is
//! true of the wire. Bytes are the one thing a CBOR API usually wants that
//! this refuses; an application that needs them describes the field itself
//! (there is no JSON Schema keyword for a CBOR byte string) and brings a codec
//! without that rule.

use std::io::Write;
use std::marker::PhantomData;

use bytes::Bytes;
use codecs::{Backend, Class};
use kynos::extract::body::codec::{BodyCodec, DecodeError};
use serde::{Serialize, de::DeserializeOwned};

/// `application/cbor` over the CBOR library `B`, held to the JSON data model.
pub struct Cbor<B>(PhantomData<fn() -> B>);

impl<B: Backend> std::fmt::Debug for Cbor<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Cbor<{}>", B::NAME)
    }
}

impl<B: Backend, T: Serialize + DeserializeOwned> BodyCodec<T> for Cbor<B> {
    const MEDIA_TYPE: &'static str = "application/cbor";

    fn decode(bytes: &Bytes) -> Result<T, DecodeError> {
        faithful::scan(bytes).map_err(|refusal| {
            if refusal.rule.is_syntax() {
                DecodeError::Syntax(refusal.to_string())
            } else {
                DecodeError::Data(refusal.to_string())
            }
        })?;
        // Cloning `Bytes` is a reference count, not a copy.
        B::decode(bytes.clone()).map_err(|failure| match failure.class {
            Class::Data => DecodeError::Data(failure.message),
            _ => DecodeError::Syntax(failure.message),
        })
    }

    fn encode(value: &T, out: &mut impl Write) -> std::io::Result<()> {
        B::to_writer(&faithful::Faithful(value), out).map_err(|failure| std::io::Error::other(failure.message))
    }
}

/// [`Cbor`] over ciborium: the library this crate's example and the README's
/// worked example use (Apache-2.0, a licence Kynos's policy accepts).
#[cfg(feature = "ciborium")]
pub type CiboriumCbor = Cbor<codecs::backend::ciborium::Ciborium>;
