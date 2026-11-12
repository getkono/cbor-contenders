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
//! Gates, against the worked codec only. On decode the gate tests
//! `faithful`'s profile, not the library: a body with no faithful schema that
//! [`faithful::scan`] lets through, or a body inside the data model it
//! refuses (what the library then makes of a body the scan passed is its
//! bare answer, recorded beside it). On encode it tests the codec's whole
//! path, the library writing through [`faithful::Faithful`]: a value with no
//! faithful schema written, one inside the data model refused, or one
//! written as another value than the JSON value serde_json writes for it
//! (unit as an empty array, say), as the oracle reads the bytes. The
//! libraries' bare answers are findings, not gates: a codec that is faithful
//! has to be written around them, and this section says where, including
//! each tag a library writes on its own.
//!
//! The types Kynos describes as formatted strings (`Uuid`, the IP address
//! types, chrono's `DateTime<Utc>` and `NaiveDate`, jiff's `Timestamp`,
//! `rust_decimal::Decimal`, `BigDecimal`) are their own table
//! ([`formatted_case`]): bare, what each library writes and whether it reads
//! the schema's string back, recorded; through the worked codec, the string
//! written and read back, gated.

use std::collections::BTreeMap;

use codecs::Backend;
use serde::Serialize;
use serde_json::{Value, json};

use crate::cases::{decode, hex};
use crate::outcome::{ARRIVALS, Outcome, Target, Typed, guarded, owned};
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
    ("a byte string into bytes::Bytes", "43 010203", "bytes-crate", None),
    ("text into Vec<u8>", "63 616263", "byte-vec", Some("string")),
    (
        "enum map keys into BTreeMap<enum, _>",
        "a1 65 416c706861 02",
        "enum-map",
        Some("object"),
    ),
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
#[derive(Debug, Serialize, serde::Deserialize, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Alpha,
}

/// A unit struct, which serde_json writes as `null`.
#[derive(Debug, Serialize)]
struct Nothing;

/// `bytes::Bytes` (its `serde` feature), rendered as hex.
#[derive(Debug)]
pub struct BytesCrate;

impl Target for BytesCrate {
    type Out<'de> = bytes::Bytes;
    fn view(out: &bytes::Bytes) -> Value {
        Value::String(hex::encode(out))
    }
}

/// The library's own decode of `bytes` into `target`: [`decode`]'s targets
/// and this section's own.
fn library<B: Backend>(target: &str, bytes: &[u8]) -> Outcome {
    match target {
        "bytes-crate" => owned::<B, BytesCrate>(bytes, ARRIVALS[0]),
        "enum-map" => owned::<B, Typed<BTreeMap<Key, u32>>>(bytes, ARRIVALS[0]),
        _ => decode::<B>(target, bytes),
    }
}

/// The tag number an encoding starts with, if it starts with one.
fn leading_tag(bytes: &[u8]) -> Option<u64> {
    let (&first, rest) = bytes.split_first()?;
    if first >> 5 != 6 {
        return None;
    }
    let width = match first & 0x1f {
        info @ 0..=23 => return Some(u64::from(info)),
        24 => 1,
        25 => 2,
        26 => 4,
        27 => 8,
        _ => return None,
    };
    let argument = rest.get(..width)?;
    Some(argument.iter().fold(0, |n, b| n << 8 | u64::from(*b)))
}

/// The encode cases: a value, its case, the library's bare encoding, the
/// worked codec's, and, where a faithful schema describes the value, the JSON
/// value it describes (what serde_json writes): the codec must write a CBOR
/// item that reads as that value.
fn encode_cases<B: Backend>() -> Vec<(&'static str, Value, Value, Option<Value>)> {
    fn bare<B: Backend, T: Serialize>(value: &T) -> Value {
        match guarded(|| B::encode(value)) {
            Ok(Ok(bytes)) => json!({
                "hex": hex::encode(&bytes),
                "major": bytes.first().map(|b| b >> 5),
                "tag": leading_tag(&bytes),
            }),
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
            let json = $faithful.then(|| serde_json::to_value(&value).expect("serde_json writes every faithful case"));
            ($what, bare::<B, _>(&value), profiled::<B, _>(&value), json)
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
        case!("1u128", 1u128, false),
        case!("u128::MAX", u128::MAX, false),
        case!("i128::MIN", i128::MIN, false),
        case!("Option::<u8>::None", None::<u8>, true),
        case!("()", (), true),
        case!("a unit struct", Nothing, true),
        case!("a unit variant", Key::Alpha, true),
        case!("serde_json::Value::Null", Value::Null, true),
        case!(
            "a serde_json::Value object holding null",
            json!({ "a": null, "b": [1, null] }),
            true
        ),
        case!("u64::MAX", u64::MAX, true),
    ]
}

/// Whether the worked codec's answer `codec` is the CBOR item for `json`:
/// written, and read by the oracle as that JSON value.
fn writes_json(codec: &Value, json: &Value) -> bool {
    codec["hex"]
        .as_str()
        .and_then(|text| hex::decode(text).ok())
        .and_then(|bytes| serde_cbor::from_slice::<Value>(&bytes).ok())
        .is_some_and(|read| &read == json)
}

/// What one formatted-string type does, bare and through the worked codec.
///
/// The form Kynos's schema describes for each is a string with a `format`
/// (`uuid`, `ipv4`, `ipv6`, `date-time`, `date`, and a decimal string), the
/// text serde_json writes for it. Bare, each library is asked what it writes
/// (its hex, whether it is a tag, whether it is a text string) and whether
/// it reads that text back as the value: recorded, since a serializer that
/// says it is not human-readable is told to write the compact form (a UUID's
/// 16 bytes, an address's octets) and many do. Through the worked codec
/// (`Faithful` around the encode, which presents a human-readable view,
/// and `scan` then `Profiled<T>` around the decode, which does too), each
/// must be written as a text string equal to that JSON string, and that
/// text must decode to the same value: gated.
fn formatted_case<B, T>(section: &mut Section, what: &str, value: &T) -> Value
where
    B: Backend,
    T: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    section.case();
    let Ok(Value::String(json)) = serde_json::to_value(value) else {
        section.fail(what, "serde_json does not write this type as a string");
        return json!({ "case": what, "json": null });
    };
    let text = payloads::cbor(&json);
    let as_text = |bytes: &[u8]| {
        serde_cbor::from_slice::<serde_cbor::Value>(bytes).ok() == Some(serde_cbor::Value::Text(json.clone()))
    };
    let reads_back = |outcome: Result<Result<T, codecs::Failure>, String>| match outcome {
        Ok(Ok(back)) if back == *value => json!("the same value"),
        Ok(Ok(back)) => json!({ "another value": format!("{back:?}") }),
        Ok(Err(failure)) => json!({ "refused": failure.status(), "message": failure.message }),
        Err(panic) => json!({ "panic": panic }),
    };
    let bare_encode = if B::ENCODES {
        match guarded(|| B::encode(value)) {
            Ok(Ok(bytes)) => json!({
                "hex": hex::encode(&bytes),
                "major": bytes.first().map(|b| b >> 5),
                "tag": leading_tag(&bytes),
                "text": as_text(&bytes),
            }),
            Ok(Err(failure)) => json!({ "refused": failure.message }),
            Err(panic) => json!({ "panic": panic }),
        }
    } else {
        Value::Null
    };
    let bare_decode = reads_back(guarded(|| B::decode::<T>(bytes::Bytes::copy_from_slice(&text))));
    let codec_encode = if B::ENCODES {
        let mut out = Vec::new();
        let written = guarded(|| B::to_writer(&faithful::Faithful(value), &mut out));
        let record = match written {
            Ok(Ok(())) => json!({ "hex": hex::encode(&out), "text": as_text(&out) }),
            Ok(Err(failure)) => json!({ "refused": failure.message }),
            Err(panic) => json!({ "panic": panic }),
        };
        if record["text"] != json!(true) {
            let case = format!("{what}#codec-encode");
            section.differ(&case, json!({ "text": json }), record.clone());
            section.fail(&case, "the worked codec does not write the string the schema describes");
        }
        record
    } else {
        Value::Null
    };
    let codec_decode = reads_back(guarded(|| {
        faithful::scan(&text)
            .map_err(|refusal| codecs::Failure::new(codecs::Class::Syntax, refusal))
            .and_then(|()| B::decode::<faithful::Profiled<T>>(bytes::Bytes::copy_from_slice(&text)))
            .map(|profiled| profiled.0)
    }));
    if codec_decode != json!("the same value") {
        let case = format!("{what}#codec-decode");
        section.differ(&case, json!("the same value"), codec_decode.clone());
        section.fail(&case, "the worked codec does not read the string the schema describes");
    }
    json!({
        "case": what,
        "json": json,
        "library": { "encode": bare_encode, "decode_text": bare_decode },
        "codec": { "encode": codec_encode, "decode_text": codec_decode },
    })
}

/// [`formatted_case`] for every type Kynos describes as a formatted string.
fn formatted<B: Backend>(section: &mut Section) -> Vec<Value> {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    let v4 = Ipv4Addr::new(192, 0, 2, 1);
    let v6 = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);
    let instant = 1_363_896_240;
    let mut out = vec![
        formatted_case::<B, _>(
            section,
            "uuid::Uuid",
            &uuid::Uuid::from_u128(0x67e5_5044_10b1_426f_9247_bb68_0e5f_e0c8),
        ),
        formatted_case::<B, _>(section, "Ipv4Addr", &v4),
        formatted_case::<B, _>(section, "Ipv6Addr", &v6),
        formatted_case::<B, _>(section, "IpAddr (V4)", &IpAddr::V4(v4)),
        formatted_case::<B, _>(section, "IpAddr (V6)", &IpAddr::V6(v6)),
    ];
    if let Some(at) = chrono::DateTime::<chrono::Utc>::from_timestamp(instant, 0) {
        out.push(formatted_case::<B, _>(section, "chrono::DateTime<Utc>", &at));
        out.push(formatted_case::<B, _>(section, "chrono::NaiveDate", &at.date_naive()));
    }
    if let Ok(at) = jiff::Timestamp::from_second(instant) {
        out.push(formatted_case::<B, _>(section, "jiff::Timestamp", &at));
    }
    out.push(formatted_case::<B, _>(
        section,
        "rust_decimal::Decimal",
        &rust_decimal::Decimal::new(12_345, 3),
    ));
    if let Ok(decimal) = "12.345".parse::<bigdecimal::BigDecimal>() {
        out.push(formatted_case::<B, _>(section, "bigdecimal::BigDecimal", &decimal));
    }
    out
}

/// Run the section.
#[must_use]
pub fn run<B: Backend>() -> Section {
    let mut section = Section::new("model");
    let mut decoded = Vec::new();
    for (what, text, target, schema) in DECODE {
        let bytes = hex(text);
        section.case();
        let library = library::<B>(target, &bytes).describe();
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
            "codec": profile.map_or_else(|r| json!({ "refused": r.status(), "rule": format!("{:?}", r.rule) }), |()| json!("passes the profile")),
        }));
    }
    let mut encoded = Vec::new();
    if B::ENCODES {
        for (what, library, codec, json) in encode_cases::<B>() {
            section.case();
            let let_through = codec.get("hex").is_some();
            let wrong = match &json {
                None if let_through => Some("the worked codec writes a value no schema describes"),
                Some(_) if !let_through => Some("the worked codec refuses a value inside the data model"),
                Some(json) if !writes_json(&codec, json) => {
                    Some("the worked codec writes another value than the one its schema describes")
                }
                _ => None,
            };
            if let Some(detail) = wrong {
                section.differ(what, json.clone().unwrap_or(Value::Null), codec.clone());
                section.fail(what, detail);
            }
            encoded.push(
                json!({ "case": what, "faithful": json.is_some(), "json": json, "library": library, "codec": codec }),
            );
        }
    }
    let formatted = formatted::<B>(&mut section);
    section.data("formatted", Value::Array(formatted));
    section.data("decode", Value::Array(decoded));
    section.data("encode", Value::Array(encoded));
    section
}

#[cfg(test)]
mod tests {
    use codecs::backend::serde_cbor::SerdeCbor;

    use super::*;

    #[test]
    fn the_worked_codec_over_the_oracle_holds_the_profile() {
        let section = run::<SerdeCbor>();
        assert_eq!(section.failure_count(), 0, "{:#?}", section.failures());
        let (record, _) = section.finish();
        let formatted = record["data"]["formatted"].as_array().cloned().unwrap_or_default();
        assert_eq!(formatted.len(), 10, "{formatted:#?}");
        // Bare, the oracle is not human-readable: a UUID is 16 bytes.
        assert_eq!(formatted[0]["library"]["encode"]["major"], 2, "{:#}", formatted[0]);
        assert_eq!(formatted[0]["json"], "67e55044-10b1-426f-9247-bb680e5fe0c8");
    }

    #[test]
    fn a_leading_tag_is_read_at_every_width() {
        assert_eq!(leading_tag(&hex("c2 41 01")), Some(2));
        assert_eq!(leading_tag(&hex("d8 63 01")), Some(0x63));
        assert_eq!(leading_tag(&hex("d9 d9f7 01")), Some(55799));
        assert_eq!(leading_tag(&hex("da 00010000 01")), Some(0x1_0000));
        assert_eq!(leading_tag(&hex("01")), None);
    }
}
