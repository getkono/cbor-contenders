//! The rule that keeps a CBOR body inside the JSON data model.
//!
//! OpenAPI 3.2 (§Parsing and Serializing) reads a non-JSON body through the
//! JSON Schema data model, and Kynos describes an `Encoded<C, T>` body with
//! the schema it derives from `T`. CBOR can carry values that model has no
//! place for, and a schema that is silent about them would be a wrong
//! contract. Kynos's `BodyCodec` trust model puts the duty on the codec: a
//! value the schema cannot describe is refused at `decode` or `encode`. This
//! crate is that refusal, written once and independent of the CBOR library
//! underneath: [`scan`] before a decode, [`Faithful`] around an encode.
//!
//! # The rules
//!
//! Decode ([`scan`]), each a [`Rule`]:
//!
//! | Input | Answer | Why |
//! | --- | --- | --- |
//! | Ill-formed (RFC 8949 §5.1): truncated, additional information 28–30, a break outside an indefinite item, an indefinite string with a chunk of another type, a two-byte simple value below 32, bytes after the item | 400 | not CBOR |
//! | A text string that is not UTF-8 (§5.3.1) | 400 | not a valid CBOR item |
//! | Nested deeper than [`MAX_DEPTH`] | 400 | the depth `Json<T>` refuses past |
//! | A byte string (major type 2) | 422 | JSON has no bytes; a `contentEncoding: base64` string is text, not a byte string, so no schema is true of it |
//! | Any tag (major type 6): date-times (0, 1), bignums (2, 3), decimal and big floats (4, 5), self-described CBOR (55799), every other | 422 | a tag has no JSON Schema form; a decoder that ignored it would accept a value the schema does not describe |
//! | `undefined` and every simple value but `false`, `true`, `null` | 422 | no JSON value |
//! | NaN, ±infinity at any width | 422 | not JSON numbers; JSON Schema `number` excludes them |
//! | A map key that is not a text string | 422 | JSON Schema object keys are strings |
//! | The same key twice in one map | 422 | RFC 8949 §5.6 calls such a map invalid; JSON Schema reads it as an object, which has one value per key |
//!
//! Accepted, because the JSON data model cannot tell them apart from their
//! preferred form (§5's last case): indefinite-length text, arrays and maps,
//! non-preferred argument widths, and floats of any width that hold a finite
//! value. Integers of any 64-bit argument are accepted: Kynos describes `u64`
//! and `i64` as `uint64`/`int64`, and a value outside the target type is the
//! target's 422.
//!
//! Encode ([`Faithful`]): a byte string (`serialize_bytes`, which
//! `serde_bytes` and `Bytes` call), a non-finite `f32`/`f64`, an `i128`/`u128`
//! (Kynos describes neither), and a map key that does not serialize as a
//! string are refused with a serde error, which a codec answers 500 with.
//! Integer map keys are what JSON would stringify; CBOR would write them as
//! integers, so the profile refuses them rather than write a key the schema
//! says is a string.

mod guard;
mod scan;

pub use guard::Faithful;
pub use scan::{MAX_DEPTH, Refusal, Rule, preferred, scan, well_formed};
