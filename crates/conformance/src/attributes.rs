//! Section `attributes`: serde's derive attributes, which route through
//! paths a plain struct never touches.
//!
//! `flatten`, `untagged` and internally tagged enums buffer the input into
//! serde's private `Content` before deciding, so they depend on exactly which
//! `visit_*` a backend calls (an integer arriving as `u64` or `f64`, a string
//! as borrowed or owned). Each type gets valid inputs, its variants, and
//! wrong shapes, written as JSON and converted to CBOR (serde_cbor, through
//! `serde_json::Value`, so an integer stays an integer and `1.0` a float).
//!
//! The reference is serde_json decoding the JSON text: serde's derive decides
//! these answers, so a self-describing format holding the same value should
//! get the same one. Every disagreement in accept/reject, value
//! (`serde_json::to_value` of the result) or status gates; a few cases that
//! fail in a non-self-describing library this way are the finding #989 asks
//! for ("record which candidates fail them and how").

use std::borrow::Cow;
use std::collections::BTreeMap;

use codecs::Backend;
use codecs::backend::serde_json::SerdeJson;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::json;

use crate::outcome::{
    ARRIVALS, Decode, OnBorrowed, Target, Typed, agree, borrowed, guarded, on_borrowed, owned, same_value, to_value,
};
use crate::record::Section;

/// `flatten` into a map.
#[derive(Debug, Serialize, Deserialize)]
pub struct Flattened {
    /// A named field.
    pub id: u32,
    /// Everything else.
    #[serde(flatten)]
    pub rest: BTreeMap<String, Value>,
}

/// `flatten` of a struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct Outer {
    /// A named field.
    pub name: String,
    /// Flattened fields.
    #[serde(flatten)]
    pub meta: Meta,
}

/// The flattened struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct Meta {
    /// An integer.
    pub version: u32,
    /// A float.
    pub weight: f64,
}

/// `untagged`: the first variant that fits wins.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Untagged {
    /// A signed integer.
    Int(i64),
    /// A float.
    Float(f64),
    /// A string.
    Text(String),
    /// A pair.
    Pair(u8, u8),
    /// A struct.
    Record {
        /// Its field.
        a: u32,
    },
}

/// Internally tagged.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Internal {
    /// Struct variant with a float.
    Circle {
        /// Radius.
        r: f64,
    },
    /// Struct variant with an integer.
    Square {
        /// Side.
        side: u32,
    },
    /// Unit variant.
    Unit,
}

/// Adjacently tagged.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum Adjacent {
    /// Newtype.
    Num(u32),
    /// Tuple.
    Pair(u8, u8),
    /// Unit.
    Unit,
    /// Struct.
    Record {
        /// Its field.
        a: bool,
    },
}

/// Externally tagged, serde's default.
#[derive(Debug, Serialize, Deserialize)]
pub enum External {
    /// Unit.
    Unit,
    /// Newtype.
    New(u32),
    /// Tuple.
    Tuple(u8, u8),
    /// Struct.
    Record {
        /// Its field.
        a: String,
    },
}

/// `alias`.
#[derive(Debug, Serialize, Deserialize)]
pub struct Aliased {
    /// Also accepted as `old_name` or `legacy`.
    #[serde(alias = "old_name", alias = "legacy")]
    pub name: String,
}

/// `rename_all = "camelCase"`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Camel {
    /// `firstName`.
    pub first_name: String,
    /// `lastSeenAt`.
    pub last_seen_at: u64,
}

fn seven() -> u8 {
    7
}

/// `default` and `skip`.
#[derive(Debug, Serialize, Deserialize)]
pub struct Defaults {
    /// Zero when absent.
    #[serde(default)]
    pub count: u32,
    /// Seven when absent.
    #[serde(default = "seven")]
    pub seven: u8,
    /// Never read.
    #[serde(skip)]
    pub skipped: u32,
    /// Required.
    pub required: bool,
}

/// `Option`, absent vs `null`.
#[derive(Debug, Serialize, Deserialize)]
pub struct Optional {
    /// Absent and `null` are both `None`.
    pub a: Option<u32>,
    /// Nested `Option`: `null` is `None`.
    pub b: Option<Option<u32>>,
}

/// `#[serde(borrow)] Cow<str>`.
#[derive(Debug, Serialize, Deserialize)]
pub struct Borrowing<'a> {
    /// A name.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// Tags.
    #[serde(borrow)]
    pub tags: Vec<Cow<'a, str>>,
}

/// [`Borrowing`] without `borrow`, for the owned decode: `Cow` then never
/// borrows.
#[derive(Debug, Serialize, Deserialize)]
pub struct Owning {
    /// A name.
    pub name: Cow<'static, str>,
    /// Tags.
    pub tags: Vec<Cow<'static, str>>,
}

/// [`Borrowing`] as a decode target.
#[derive(Debug)]
pub struct BorrowingTarget;

impl Target for BorrowingTarget {
    type Out<'de> = Borrowing<'de>;
    fn view(out: &Borrowing<'_>) -> Value {
        to_value(out)
    }
}

/// `&str` fields, borrowed in every position: only a library that hands out
/// a slice of the input decodes one.
#[derive(Debug, Serialize, Deserialize)]
pub struct BorrowingStr<'a> {
    /// A name.
    pub name: &'a str,
    /// Tags.
    #[serde(borrow)]
    pub tags: Vec<&'a str>,
}

/// [`BorrowingStr`], owned.
#[derive(Debug, Serialize, Deserialize)]
pub struct OwningStr {
    /// A name.
    pub name: String,
    /// Tags.
    pub tags: Vec<String>,
}

/// [`BorrowingStr`] as a decode target.
#[derive(Debug)]
pub struct BorrowingStrTarget;

impl Target for BorrowingStrTarget {
    type Out<'de> = BorrowingStr<'de>;
    fn view(out: &BorrowingStr<'_>) -> Value {
        to_value(out)
    }
}

/// The encode side of a case: the value serde_json reads from its JSON,
/// encoded by the library and read back by the library and the oracle.
type Encode = fn(&str) -> Option<Encoded>;

/// What the encode side found.
#[derive(Debug)]
pub struct Encoded {
    /// The report's record.
    pub record: Value,
    /// Why it gates, if it does.
    pub failure: Option<String>,
}

/// Encode the value `input` holds as a `T` (none, where serde_json refuses
/// it: there is no value to encode), and check it reads back as itself.
fn encode_side<B: Backend, T: Serialize + DeserializeOwned>(input: &str) -> Option<Encoded> {
    let value: T = serde_json::from_str(input).ok()?;
    let original = to_value(&value);
    let bytes = match guarded(|| B::encode(&value)) {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(failure)) => {
            return Some(Encoded {
                record: json!({ "encode": { "refused": failure.to_string() } }),
                failure: Some(format!("encode failed: {failure}")),
            });
        }
        Err(panic) => {
            return Some(Encoded {
                record: json!({ "encode": { "panic": panic } }),
                failure: Some(format!("encode panicked: {panic}")),
            });
        }
    };
    let own = guarded(|| B::decode::<T>(bytes::Bytes::copy_from_slice(&bytes)))
        .map_err(|panic| format!("panicked: {panic}"))
        .and_then(|result| result.map_err(|failure| failure.to_string()))
        .map(|back| to_value(&back));
    let oracle = serde_cbor::from_slice::<T>(&bytes)
        .map_err(|error| error.to_string())
        .map(|back| to_value(&back));
    let well_formed = faithful::well_formed(&bytes);
    let failure = match (&well_formed, &own, &oracle) {
        (Err(refusal), _, _) => Some(format!("the encoding is not one well-formed item: {refusal}")),
        (_, Err(why), _) => Some(format!("the library does not read its own encoding back: {why}")),
        (_, _, Err(why)) => Some(format!("the oracle does not read the encoding: {why}")),
        (_, Ok(own), _) if !same_value(own, &original) => {
            Some("the library reads its own encoding back as another value".to_owned())
        }
        (_, _, Ok(oracle)) if !same_value(oracle, &original) => {
            Some("the oracle reads the encoding as another value".to_owned())
        }
        _ => None,
    };
    Some(Encoded {
        record: json!({
            "hex": crate::outcome::abbreviate(&json!(hex::encode(&bytes))),
            "preferred": faithful::preferred(&bytes),
            // `flatten` serializes through `serialize_map(None)`: an encoder
            // that writes what it is told writes an indefinite map, `bf`.
            "indefinite_root": bytes.first().is_some_and(|b| matches!(b, 0x5f | 0x7f | 0x9f | 0xbf)),
            "own": own.map_or_else(|why| json!({ "refused": why }), |_| json!("read back")),
            "oracle": oracle.map_or_else(|why| json!({ "refused": why }), |_| json!("read back")),
        }),
        failure,
    })
}

/// A type's name, its owned and borrowed decode paths, its encode side, and
/// its inputs.
type Case = (
    &'static str,
    [(&'static str, Decode, Decode); 2],
    Encode,
    &'static [&'static str],
);

/// Each type's decode paths and inputs.
#[expect(clippy::too_many_lines, reason = "a table of inputs")]
fn cases<B: Backend>() -> Vec<Case> {
    macro_rules! typed {
        ($name:literal, $ty:ty, $inputs:expr) => {
            (
                $name,
                [
                    (
                        "owned",
                        owned::<B, Typed<$ty>> as Decode,
                        owned::<SerdeJson, Typed<$ty>> as Decode,
                    ),
                    (
                        "borrowed",
                        borrowed::<B, Typed<$ty>>,
                        borrowed::<SerdeJson, Typed<$ty>>,
                    ),
                ],
                encode_side::<B, $ty> as Encode,
                $inputs,
            )
        };
    }
    vec![
        typed!(
            "flatten-map",
            Flattened,
            &[
                r#"{"id":1}"#,
                r#"{"id":1,"x":1.5,"y":[1,"a"],"z":{"q":null}}"#,
                r#"{"x":-1,"id":2,"big":18446744073709551615}"#,
                r#"{"x":1}"#,
                r#"{"id":"1"}"#,
                "[1]",
            ]
        ),
        typed!(
            "flatten-struct",
            Outer,
            &[
                r#"{"name":"n","version":1,"weight":0.5}"#,
                r#"{"version":1,"weight":2,"name":"n"}"#,
                r#"{"name":"n","version":1}"#,
                r#"{"name":"n","version":-1,"weight":0.5}"#,
                r#"{"name":"n","version":1,"weight":0.5,"extra":true}"#,
            ]
        ),
        typed!(
            "untagged",
            Untagged,
            &[
                "1",
                "-1",
                "1.5",
                "1.0",
                "18446744073709551615",
                "1e2",
                r#""x""#,
                "[1,2]",
                "[1,256]",
                r#"{"a":1}"#,
                "true",
                "null",
            ]
        ),
        typed!(
            "internally-tagged",
            Internal,
            &[
                r#"{"type":"Circle","r":1.5}"#,
                r#"{"r":1,"type":"Circle"}"#,
                r#"{"type":"Square","side":3}"#,
                r#"{"type":"Unit"}"#,
                r#"{"type":"Hexagon"}"#,
                r#"{"r":1.5}"#,
                r#"{"type":"Square","side":-1}"#,
                r#"["Circle",1.5]"#,
            ]
        ),
        typed!(
            "adjacently-tagged",
            Adjacent,
            &[
                r#"{"t":"Num","c":1}"#,
                r#"{"c":[1,2],"t":"Pair"}"#,
                r#"{"t":"Unit"}"#,
                r#"{"t":"Record","c":{"a":true}}"#,
                r#"{"t":"Num"}"#,
                r#"{"t":"Nope","c":1}"#,
                r#"{"t":"Num","c":"1"}"#,
            ]
        ),
        typed!(
            "externally-tagged",
            External,
            &[
                r#""Unit""#,
                r#"{"New":1}"#,
                r#"{"Tuple":[1,2]}"#,
                r#"{"Record":{"a":"x"}}"#,
                r#"{"Unit":null}"#,
                r#""Nope""#,
                r#"{"New":1,"Unit":null}"#,
                r#"{"Tuple":[1]}"#,
            ]
        ),
        typed!(
            "alias",
            Aliased,
            &[
                r#"{"name":"a"}"#,
                r#"{"old_name":"a"}"#,
                r#"{"legacy":"a"}"#,
                r#"{"name":"a","legacy":"b"}"#,
                r#"{"other":"a"}"#,
            ]
        ),
        typed!(
            "rename-all",
            Camel,
            &[
                r#"{"firstName":"a","lastSeenAt":1}"#,
                r#"{"first_name":"a","last_seen_at":1}"#,
                r#"{"firstName":"a"}"#,
                r#"{"firstName":1,"lastSeenAt":1}"#,
            ]
        ),
        typed!(
            "default-skip",
            Defaults,
            &[
                r#"{"required":true}"#,
                r#"{"count":3,"seven":1,"required":false}"#,
                r#"{"skipped":5,"required":true}"#,
                r#"{"count":null,"required":true}"#,
                "{}",
            ]
        ),
        typed!(
            "option",
            Optional,
            &["{}", r#"{"a":null,"b":null}"#, r#"{"a":1,"b":2}"#, r#"{"a":"1"}"#,]
        ),
        (
            "borrow-cow",
            [
                (
                    "owned",
                    owned::<B, Typed<Owning>> as Decode,
                    owned::<SerdeJson, Typed<Owning>> as Decode,
                ),
                (
                    "borrowed",
                    borrowed::<B, BorrowingTarget>,
                    borrowed::<SerdeJson, BorrowingTarget>,
                ),
            ],
            encode_side::<B, Owning>,
            &[
                r#"{"name":"plain","tags":["a","b"]}"#,
                r#"{"name":"esc\"aped","tags":["\u00e9","\n"]}"#,
                r#"{"name":1,"tags":[]}"#,
                r#"{"name":"x"}"#,
            ],
        ),
        // No escapes: serde_json borrows every one of these strings, so its
        // answer is the reference on both paths.
        (
            "borrow-str",
            [
                (
                    "owned",
                    owned::<B, Typed<OwningStr>> as Decode,
                    owned::<SerdeJson, Typed<OwningStr>> as Decode,
                ),
                (
                    "borrowed",
                    borrowed::<B, BorrowingStrTarget>,
                    borrowed::<SerdeJson, BorrowingStrTarget>,
                ),
            ],
            encode_side::<B, OwningStr>,
            &[
                r#"{"name":"plain","tags":["a","b"]}"#,
                "{\"name\":\"\u{e9}t\u{e9}\",\"tags\":[]}",
                r#"{"name":1,"tags":[]}"#,
                r#"{"name":"x"}"#,
            ],
        ),
    ]
}

/// A case's JSON input as CBOR.
fn cbor_of(input: &str) -> Vec<u8> {
    let value: Value = serde_json::from_str(input).unwrap_or_else(|e| unreachable!("a case is JSON: {e}"));
    payloads::cbor(&value)
}

/// Run the section.
///
/// Decode: the owned path gates every disagreement with serde_json. The
/// borrowed path is judged by [`on_borrowed`]: a library that decodes an
/// input owned but refuses it borrowed does not borrow that target, which is
/// recorded as not borrowed; every other disagreement gates.
///
/// Encode: each value serde_json reads from a case's JSON is encoded by the
/// library, which must write one well-formed item that it and the oracle
/// both read back as the same value. Whether the bytes are preferred
/// serialization, and whether the root is indefinite-length (`flatten`
/// serializes through `serialize_map(None)`), is recorded.
#[must_use]
pub fn run<B: Backend>() -> Section {
    let mut section = Section::new("attributes");
    let mut encoded = serde_json::Map::new();
    for (type_name, paths, encode, inputs) in cases::<B>() {
        let [(_, owned_b, owned_r), (_, borrowed_b, borrowed_r)] = paths;
        for (index, input) in inputs.iter().enumerate() {
            let cbor = cbor_of(input);
            for arrival in ARRIVALS {
                let owned = owned_b(&cbor, arrival);
                let name = format!("{type_name}/{index}#owned#{}", arrival.name());
                let r = owned_r(input.as_bytes(), arrival);
                section.case();
                if !agree(&r, &owned) {
                    section.differ(
                        &name,
                        json!({ "input": input, "outcome": r.describe() }),
                        owned.describe(),
                    );
                    section.fail(&name, "outcome differs from serde_json on the same value as JSON");
                }
                let name = format!("{type_name}/{index}#borrowed#{}", arrival.name());
                let r = borrowed_r(input.as_bytes(), arrival);
                let b = borrowed_b(&cbor, arrival);
                match on_borrowed(&b, !owned.is_accept(), agree(&r, &b)) {
                    OnBorrowed::Right => section.case(),
                    OnBorrowed::NotBorrowed => section.not_borrowed(&name, b.describe()),
                    OnBorrowed::Wrong => {
                        section.case();
                        section.differ(&name, json!({ "input": input, "outcome": r.describe() }), b.describe());
                        section.fail(&name, "outcome differs from serde_json on the same value as JSON");
                    }
                }
            }
            if !B::ENCODES {
                continue;
            }
            if let Some(side) = encode(input) {
                let name = format!("{type_name}/{index}#encode");
                section.case();
                if let Some(failure) = side.failure {
                    section.differ(&name, json!({ "input": input }), side.record.clone());
                    section.fail(&name, failure);
                }
                encoded.insert(name, side.record);
            }
        }
    }
    let preferred = encoded.values().all(|record| record["preferred"] == json!(true));
    section.data("encode", json!({ "all_preferred": preferred, "by_case": encoded }));
    section
}
