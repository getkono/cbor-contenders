//! Section `model`: what each library does with the CBOR values the JSON data
//! model has no place for (#989 section 5), and what the worked codec does.
//!
//! OpenAPI 3.2 reads a non-JSON body through the JSON Schema data model, and
//! the schema Kynos derives from `T` describes only that model. For each case
//! this records the library's own answer, bare (decoded into the type a
//! handler would declare, or encoded from it), and the worked codec's
//! (`kynos-cbor`: [`faithful::scan`] before a decode, [`faithful::Faithful`]
//! around an encode).
//!
//! Gates, against the worked codec only: a case with no faithful schema that
//! its profile lets through, or a case inside the data model that it
//! refuses. The libraries' bare answers are findings, not gates: a codec that
//! is faithful has to be written around them, and this section says where.

use std::collections::BTreeMap;

use codecs::Backend;
use serde::Serialize;
use serde_json::{Value, json};

use crate::cases::{decode, hex};
use crate::outcome::guarded;
use crate::record::Section;

/// `(case, body, the target a handler would declare, what a faithful schema
/// says)`: `None` where no schema Kynos derives is true of the body.
const DECODE: &[(&str, &str, &str, Option<&str>)] = &[
    ("a byte string into Vec<u8>", "43 010203", "byte-vec", None),
    ("a byte string into serde_bytes", "43 010203", "bytes", None),
    (
        "an array of integers into Vec<u8>",
        "83 01 02 03",
        "byte-vec",
        Some("array of uint8"),
    ),
    (
        "an array of integers into serde_bytes",
        "83 01 02 03",
        "bytes",
        Some("array of uint8"),
    ),
    ("text into serde_bytes", "63 616263", "bytes", Some("string")),
    (
        "integer map keys into BTreeMap<u64, _>",
        "a2 01 02 03 04",
        "int-map",
        None,
    ),
    (
        "integer map keys into BTreeMap<String, _>",
        "a1 01 02",
        "text-map",
        None,
    ),
    (
        "tag 0 date-time text into String",
        "c0 74 323031332d30332d32315432303a30343a30305a",
        "string",
        None,
    ),
    ("tag 1 epoch integer into u64", "c1 1a 514b67b0", "u64", None),
    ("tag 1 epoch float into f64", "c1 fb 41d452d9ec200000", "f64", None),
    ("tag 2 bignum into u128", "c2 49 010000000000000000", "u128", None),
    ("tag 2 small bignum into u64", "c2 41 01", "u64", None),
    ("tag 3 bignum into i128", "c3 49 010000000000000000", "i128", None),
    ("tag 4 decimal fraction into a value", "c4 82 21 19 6ab3", "value", None),
    ("tag 5 bigfloat into a value", "c5 82 20 03", "value", None),
    ("tag 55799 self-described CBOR into u64", "d9 d9f7 01", "u64", None),
    ("an unknown tag into u64", "d8 63 01", "u64", None),
    ("undefined into Option<u8>", "f7", "option", None),
    ("simple(16) into a value", "f0", "value", None),
    ("NaN into f64", "f9 7e00", "f64", None),
    ("+infinity into f64", "f9 7c00", "f64", None),
    ("-infinity into f32", "fa ff800000", "f32", None),
    (
        "2^53 + 1 into u64",
        "1b 0020000000000001",
        "u64",
        Some("integer, uint64"),
    ),
    (
        "u64::MAX into u64",
        "1b ffffffffffffffff",
        "u64",
        Some("integer, uint64"),
    ),
    (
        "indefinite text into String",
        "7f 61 61 61 62 ff",
        "string",
        Some("string"),
    ),
    (
        "an indefinite array into a value",
        "9f 01 02 ff",
        "value",
        Some("array"),
    ),
    (
        "an indefinite map into BTreeMap<String, _>",
        "bf 61 61 01 ff",
        "text-map",
        Some("object"),
    ),
    (
        "a non-preferred head into u64",
        "1b 0000000000000001",
        "u64",
        Some("integer"),
    ),
    (
        "a double that fits a half into f64",
        "fb 3ff8000000000000",
        "f64",
        Some("number"),
    ),
];

/// A map key that is an enum's unit variant, written as its name.
#[derive(Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Alpha,
}

/// The encode cases: a value, its case, and whether a faithful schema
/// describes what a JSON-model encoder must write for it.
fn encode_cases<B: Backend>() -> Vec<(&'static str, Value, Value, bool)> {
    fn bare<B: Backend, T: Serialize>(value: &T) -> Value {
        match guarded(|| B::encode(value)) {
            Ok(Ok(bytes)) => json!({ "hex": hex::encode(&bytes), "major": bytes.first().map(|b| b >> 5) }),
            Ok(Err(failure)) => json!({ "refused": failure.message }),
            Err(panic) => json!({ "panic": panic }),
        }
    }
    fn profiled<B: Backend, T: Serialize>(value: &T) -> Value {
        let mut out = Vec::new();
        match guarded(|| B::to_writer(&faithful::Faithful(value), &mut out)) {
            Ok(Ok(())) => json!({ "hex": hex::encode(&out) }),
            Ok(Err(failure)) => json!({ "refused": failure.message }),
            Err(panic) => json!({ "panic": panic }),
        }
    }
    macro_rules! case {
        ($what:literal, $value:expr, $faithful:expr) => {{
            let value = $value;
            ($what, bare::<B, _>(&value), profiled::<B, _>(&value), $faithful)
        }};
    }
    vec![
        case!("Vec<u8>", vec![1u8, 2, 3], true),
        case!(
            "serde_bytes::ByteBuf",
            serde_bytes::ByteBuf::from(vec![1u8, 2, 3]),
            false
        ),
        case!("bytes::Bytes", bytes::Bytes::from_static(&[1, 2, 3]), false),
        case!("BTreeMap<u64, u32>", BTreeMap::from([(1u64, 2u32)]), false),
        case!("BTreeMap<enum, u32>", BTreeMap::from([(Key::Alpha, 2u32)]), true),
        case!("f64::NAN", f64::NAN, false),
        case!("f64::INFINITY", f64::INFINITY, false),
        case!("u128::MAX", u128::MAX, false),
        case!("i128::MIN", i128::MIN, false),
        case!("Option::<u8>::None", None::<u8>, true),
        case!("()", (), true),
        case!("u64::MAX", u64::MAX, true),
    ]
}

/// Run the section.
#[must_use]
pub fn run<B: Backend>() -> Section {
    let mut section = Section::new("model");
    let mut decoded = Vec::new();
    for (what, text, target, schema) in DECODE {
        let bytes = hex(text);
        section.case();
        let library = decode::<B>(target, &bytes).describe();
        let profile = faithful::scan(&bytes);
        let refused = profile.is_err();
        if refused == schema.is_some() {
            let detail = if refused {
                "the worked codec refuses a value inside the data model"
            } else {
                "the worked codec lets through a value no schema describes"
            };
            section.differ(what, json!(schema), json!(format!("{profile:?}")));
            section.fail(what, detail);
        }
        decoded.push(json!({
            "case": what,
            "hex": text,
            "target": target,
            "schema": schema,
            "library": library,
            "codec": profile.map_or_else(|r| json!({ "refused": r.status(), "rule": format!("{:?}", r.rule) }), |()| json!("decodes")),
        }));
    }
    let mut encoded = Vec::new();
    if B::ENCODES {
        for (what, library, codec, schema) in encode_cases::<B>() {
            section.case();
            let let_through = codec.get("hex").is_some();
            if let_through != schema {
                section.differ(what, json!(schema), codec.clone());
                section.fail(
                    what,
                    if let_through {
                        "the worked codec writes a value no schema describes"
                    } else {
                        "the worked codec refuses a value inside the data model"
                    },
                );
            }
            encoded.push(json!({ "case": what, "faithful": schema, "library": library, "codec": codec }));
        }
    }
    section.data("decode", Value::Array(decoded));
    section.data("encode", Value::Array(encoded));
    section
}
