//! The rule that keeps a CBOR body inside the JSON data model.
//!
//! OpenAPI 3.2 (§Parsing and Serializing) reads a non-JSON body through the
//! JSON Schema data model, and Kynos describes an `Encoded<C, T>` body with
//! the schema it derives from `T`. CBOR can carry values that model has no
//! place for, and a schema that is silent about them would be a wrong
//! contract. Kynos's `BodyCodec` trust model puts the duty on the codec: a
//! value the schema cannot describe is refused at `decode` or `encode`. This
//! crate is that refusal, written once and independent of the CBOR library
//! underneath: [`scan`] before a decode, [`Profiled`] around the decoded
//! type, [`Faithful`] around an encode.
//!
//! # The rules
//!
//! Decode ([`scan`]), each a [`Rule`]. A body that is not CBOR anywhere is
//! a 400, even where a value the data model refuses comes first: the first
//! 422 is held until the whole body is known to be CBOR.
//!
//! | Input | Answer | Why |
//! | --- | --- | --- |
//! | Ill-formed (RFC 8949 §5.1): truncated, additional information 28–30, an indefinite length on an integer or a tag, a break outside an indefinite item, a tag followed by a break instead of its content, an indefinite string with a chunk of another type, a two-byte simple value below 32, bytes after the item | 400 | not CBOR |
//! | A text string, or a chunk of an indefinite one (a key's included), that is not UTF-8 (§3.2.3, §5.3.1) | 400 | not a valid CBOR item |
//! | More than [`MAX_DEPTH`] (127) arrays and maps nested, empty ones included | 400 | `Json<T>` reads 127 levels and refuses the 128th |
//! | A byte string (major type 2) | 422 | JSON has no bytes; a `contentEncoding: base64` string is text, not a byte string, so no schema is true of it |
//! | Any tag (major type 6): date-times (0, 1), bignums (2, 3), decimal and big floats (4, 5), self-described CBOR (55799), every other | 422 | a tag has no JSON Schema form; a decoder that ignored it would accept a value the schema does not describe |
//! | `undefined` and every simple value but `false`, `true`, `null` | 422 | no JSON value |
//! | NaN, ±infinity at any width | 422 | not JSON numbers; JSON Schema `number` excludes them |
//! | A map key that is not a text string | 422 | JSON Schema object keys are strings |
//! | The same key twice in one map | 422 | RFC 8949 §5.6 calls such a map invalid; JSON Schema reads it as an object, which has one value per key |
//!
//! Decode ([`Profiled`]), around the type:
//!
//! | Input | Answer | Why |
//! | --- | --- | --- |
//! | A struct, or a struct variant, read from an array | 422 | the schema says `type: object`, and `Json<T>` refuses it |
//! | A `Uuid`, an address, a date-time or a decimal as the text its schema's `format` names | accepted | the human-readable form, which the schema describes |
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
//!
//! Both halves present a human-readable serde view ([`Profiled`] on decode,
//! [`Faithful`] on encode), so a type whose serde impl switches on
//! `is_human_readable` (`Uuid`, the IP address types) is read and written
//! as the string Kynos's schema describes, as `Json<T>` reads and writes it.
//! [`Profiled`] also refuses a struct read from an array, which serde's
//! derive allows and `Json<T>` refuses: 422.

mod guard;
mod profiled;
mod scan;

pub use guard::Faithful;
pub use profiled::Profiled;
pub use scan::{MAX_DEPTH, Refusal, Rule, preferred, scan, well_formed};
