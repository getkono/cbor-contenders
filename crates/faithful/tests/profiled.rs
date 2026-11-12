//! The decode view, through serde_cbor (which says it is not
//! human-readable and reads a struct from an array): every position it
//! hands out is human-readable, and a struct is read only from a map.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use faithful::Profiled;
use serde::de::{DeserializeOwned, IgnoredAny};
use serde::{Deserialize, Deserializer, Serialize};
use serde_cbor::Value;

fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, serde_cbor::Error> {
    serde_cbor::from_slice::<Profiled<T>>(bytes).map(|p| p.0)
}

fn cbor<T: Serialize>(value: &T) -> Vec<u8> {
    serde_cbor::to_vec(value).expect("encodes")
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn map(entries: Vec<(Value, Value)>) -> Value {
    Value::Map(entries.into_iter().collect())
}

/// Records what its deserializer says, wherever it is.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Readable(bool);

impl<'de> Deserialize<'de> for Readable {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let readable = deserializer.is_human_readable();
        IgnoredAny::deserialize(deserializer)?;
        Ok(Self(readable))
    }
}

#[derive(Debug, PartialEq, Deserialize)]
enum Shape {
    Unit,
    Newtype(Readable),
    Tuple(Readable, Readable),
    Struct { inner: Readable },
}

#[derive(Debug, PartialEq, Deserialize)]
struct Everywhere {
    field: Readable,
    list: Vec<Readable>,
    option: Option<Readable>,
    values: BTreeMap<String, Readable>,
    keys: BTreeMap<Readable, u8>,
    shapes: Vec<Shape>,
    nested: Vec<Option<Vec<Readable>>>,
}

#[test]
fn every_position_is_human_readable() {
    let one = || Value::Integer(1);
    let body = cbor(&map(vec![
        (text("field"), one()),
        (text("list"), Value::Array(vec![one()])),
        (text("option"), one()),
        (text("values"), map(vec![(text("a"), one())])),
        (text("keys"), map(vec![(text("k"), one())])),
        (
            text("shapes"),
            Value::Array(vec![
                text("Unit"),
                map(vec![(text("Newtype"), one())]),
                map(vec![(text("Tuple"), Value::Array(vec![one(), one()]))]),
                map(vec![(text("Struct"), map(vec![(text("inner"), one())]))]),
            ]),
        ),
        (text("nested"), Value::Array(vec![Value::Array(vec![one()])])),
    ]));
    let bare: Everywhere = serde_cbor::from_slice(&body).expect("bare");
    assert_eq!(bare.field, Readable(false));
    let seen: Everywhere = decode(&body).expect("profiled");
    assert_eq!(
        seen,
        Everywhere {
            field: Readable(true),
            list: vec![Readable(true)],
            option: Some(Readable(true)),
            values: BTreeMap::from([("a".into(), Readable(true))]),
            keys: BTreeMap::from([(Readable(true), 1)]),
            shapes: vec![
                Shape::Unit,
                Shape::Newtype(Readable(true)),
                Shape::Tuple(Readable(true), Readable(true)),
                Shape::Struct { inner: Readable(true) },
            ],
            nested: vec![Some(vec![Readable(true)])],
        }
    );
}

#[test]
fn a_formatted_string_is_read_as_its_schema_describes() {
    let address = Ipv4Addr::new(192, 0, 2, 1);
    let body = cbor(&"192.0.2.1");
    assert!(
        serde_cbor::from_slice::<Ipv4Addr>(&body).is_err(),
        "bare, it wants octets"
    );
    assert_eq!(decode::<Ipv4Addr>(&body).expect("text"), address);
    assert_eq!(decode::<IpAddr>(&body).expect("text"), IpAddr::V4(address));
    let keyed = cbor(&BTreeMap::from([("192.0.2.1", vec!["::1"])]));
    assert_eq!(
        decode::<BTreeMap<Ipv4Addr, Vec<IpAddr>>>(&keyed).expect("text"),
        BTreeMap::from([(address, vec![IpAddr::V6(Ipv6Addr::LOCALHOST)])])
    );
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Record {
    id: u64,
    name: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Pair(u64, String);

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Variant {
    Struct { id: u64, name: String },
    Tuple(u64, String),
}

#[test]
fn a_struct_is_read_only_from_a_map() {
    let array = cbor(&(7u64, "seven"));
    assert!(
        serde_cbor::from_slice::<Record>(&array).is_ok(),
        "bare, the derive reads an array"
    );
    let error = decode::<Record>(&array).expect_err("an array is not an object");
    assert!(error.to_string().contains("invalid type: sequence"), "{error}");
    assert!(decode::<Vec<Record>>(&cbor(&vec![(7u64, "seven")])).is_err());
    assert!(decode::<Option<Record>>(&array).is_err());
    let variant = cbor(&BTreeMap::from([("Struct", (7u64, "seven"))]));
    assert!(decode::<Variant>(&variant).is_err());
    // A map still reads, and a tuple struct or tuple variant still reads
    // from an array.
    let record = Record {
        id: 7,
        name: "seven".into(),
    };
    assert_eq!(decode::<Record>(&cbor(&record)).expect("a map"), record);
    assert_eq!(decode::<Pair>(&array).expect("a tuple struct"), Pair(7, "seven".into()));
    for value in [
        Variant::Tuple(7, "seven".into()),
        Variant::Struct {
            id: 7,
            name: "seven".into(),
        },
    ] {
        assert_eq!(decode::<Variant>(&cbor(&value)).expect("a variant"), value);
    }
}

#[test]
fn borrowing_and_flattening_pass_through() {
    #[derive(Debug, PartialEq, Deserialize)]
    struct Borrowed<'a> {
        #[serde(borrow)]
        name: &'a str,
        #[serde(flatten)]
        rest: BTreeMap<String, u64>,
    }
    let body = cbor(&map(vec![(text("name"), text("n")), (text("x"), Value::Integer(1))]));
    let Profiled(seen) = serde_cbor::from_slice::<Profiled<Borrowed<'_>>>(&body).expect("decodes");
    assert_eq!(seen.name, "n");
    assert!(
        body.as_ptr_range().contains(&seen.name.as_ptr()),
        "borrowed from the body"
    );
    assert_eq!(seen.rest, BTreeMap::from([("x".into(), 1)]));
}
