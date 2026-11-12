//! The worked codec through Kynos, for every CBOR library this build has:
//! each row of the codec's table (`src/lib.rs`), as a client sees it.
//!
//! These test the codec's rules, not the libraries: the record's float needs
//! a double (so no library's half- or single-width support is in play) and
//! no body uses an encoding a library may refuse on its own. What each
//! library does with those is measured by conformance.
#![cfg(any(
    feature = "ciborium",
    feature = "minicbor-serde",
    feature = "cbor4ii",
    feature = "cbor2",
    feature = "c2pa_cbor"
))]

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};

use kynos::{
    error::rejection::NegotiationRejection,
    extract::body::{codec::Encoded, limit::DEFAULT_LIMIT},
    http::StatusCode,
    prelude::*,
    response::{
        negotiate::{Accept, Negotiated},
        status::NoContent,
    },
    schema::unchecked::Unchecked,
    test::TestClient,
};
use serde::{Deserialize, Serialize};

/// A record with every JSON type.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Record {
    id: u64,
    name: String,
    tags: Vec<String>,
    score: f64,
    parent: Option<u64>,
    labels: BTreeMap<String, u32>,
}

/// Two members, which serde's derive would also read from a two-element
/// array.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Pair {
    id: u64,
    name: String,
}

/// A member whose schema declares a bound.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Bounded {
    #[schema(maximum = 10)]
    count: u64,
}

/// Every type Kynos describes as a formatted string. `Uuid` and the
/// addresses write that string only to a human-readable format; the
/// date-times and the decimal write it to every format.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Formatted {
    id: uuid::Uuid,
    v4: Ipv4Addr,
    ip: IpAddr,
    at: chrono::DateTime<chrono::Utc>,
    when: jiff::Timestamp,
    amount: rust_decimal::Decimal,
}

/// The JSON form of a [`Formatted`], as `Json<T>` reads it.
fn formatted_json() -> serde_json::Value {
    serde_json::json!({
        "id": "67e55044-10b1-426f-9247-bb680e5fe0c8",
        "v4": "192.0.2.1",
        "ip": "2001:db8::1",
        "at": "2026-11-11T12:00:00Z",
        "when": "2026-11-11T12:00:00Z",
        "amount": "12.50",
    })
}

/// `levels` arrays, nested, in JSON.
fn nested_json(levels: usize) -> Vec<u8> {
    format!("{}{}", "[".repeat(levels), "]".repeat(levels)).into_bytes()
}

/// `levels` arrays, nested, in CBOR.
fn nested_cbor(levels: usize) -> Vec<u8> {
    std::iter::repeat_n(0x81, levels - 1).chain([0x80]).collect()
}

/// A response carrying a float the handler chooses.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Reading {
    value: f64,
}

fn record() -> Record {
    Record {
        id: 7,
        name: "seven".into(),
        tags: vec!["a".into(), "b".into()],
        score: 0.1,
        parent: None,
        labels: BTreeMap::from([("k".into(), 1)]),
    }
}

fn hex(text: &str) -> Vec<u8> {
    let text: String = text.split_whitespace().collect();
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect()
}

/// `record()` with one member's value replaced by raw CBOR, spliced in
/// byte for byte (a value type would drop a tag).
fn record_with(member: &str, raw: &str) -> Vec<u8> {
    const MARK: &str = "\u{1}placeholder\u{1}";
    let mut map = serde_cbor::value::to_value(record()).expect("a value");
    let serde_cbor::Value::Map(entries) = &mut map else {
        unreachable!()
    };
    entries.insert(
        serde_cbor::Value::Text(member.into()),
        serde_cbor::Value::Text(MARK.into()),
    );
    let encoded = serde_cbor::to_vec(&map).expect("encodes");
    let mark = serde_cbor::to_vec(&MARK).expect("encodes");
    let at = encoded
        .windows(mark.len())
        .position(|w| w == mark.as_slice())
        .expect("the mark is written");
    [&encoded[..at], &hex(raw)[..], &encoded[at + mark.len()..]].concat()
}

macro_rules! suite {
    ($($feature:literal => $module:ident: $backend:ty),* $(,)?) => {$(
        #[cfg(feature = $feature)]
        mod $module {
            use super::*;

            type Cbor = kynos_cbor::Cbor<$backend>;
            type Formats = (Json<Record>, Encoded<Cbor, Record>);

            #[kynos::post("/records")]
            async fn echo(record: Encoded<Cbor, Record>) -> Encoded<Cbor, Record> {
                record
            }

            #[kynos::get("/records")]
            async fn negotiated(accept: Accept<Formats>) -> Result<Negotiated<Formats>, NegotiationRejection> {
                accept.respond_with(
                    &record(),
                    (|r: &Record| Json(r.clone()), |r: &Record| Encoded::new(r.clone())),
                )
            }

            #[kynos::get("/nan")]
            async fn nan() -> Encoded<Cbor, Reading> {
                Encoded::new(Reading { value: f64::NAN })
            }

            #[kynos::post("/pairs")]
            async fn pairs(_: Encoded<Cbor, Pair>) -> NoContent {
                NoContent
            }

            #[kynos::post("/pairs.json")]
            async fn pairs_as_json(_: Json<Pair>) -> NoContent {
                NoContent
            }

            #[kynos::post("/bounded")]
            async fn bounded(_: Encoded<Cbor, Bounded>) -> NoContent {
                NoContent
            }

            #[kynos::post("/formatted")]
            async fn formatted(value: Encoded<Cbor, Formatted>) -> Encoded<Cbor, Formatted> {
                value
            }

            #[kynos::post("/nested")]
            async fn nested(_: Encoded<Cbor, Unchecked<serde_json::Value>>) -> NoContent {
                NoContent
            }

            #[kynos::post("/nested.json")]
            async fn nested_as_json(_: Json<Unchecked<serde_json::Value>>) -> NoContent {
                NoContent
            }

            fn client() -> TestClient<()> {
                let router = Router::<()>::new().mount(kynos::routes![
                    echo,
                    negotiated,
                    nan,
                    pairs,
                    pairs_as_json,
                    bounded,
                    formatted,
                    nested,
                    nested_as_json
                ]);
                TestClient::new(router.build(()).expect("builds"))
            }

            async fn post_to(client: &TestClient<()>, path: &str, body: Vec<u8>) -> (StatusCode, bytes::Bytes) {
                let response = client.post(path).body("application/cbor", body).send().await;
                (response.status(), response.bytes().clone())
            }

            async fn post(client: &TestClient<()>, body: Vec<u8>) -> (StatusCode, bytes::Bytes) {
                let response = client.post("/records").body("application/cbor", body).send().await;
                (response.status(), response.bytes().clone())
            }

            #[tokio::test]
            async fn a_record_round_trips_and_the_description_states_it() {
                let client = client();
                let body = serde_cbor::to_vec(&record()).expect("encodes");
                let response = client.post("/records").body("application/cbor", body).send().await;
                response.assert_status(StatusCode::OK).assert_header("content-type", "application/cbor");
                let back: Record = serde_cbor::from_slice(response.bytes()).expect("CBOR");
                assert_eq!(back, record());
                assert_eq!(faithful::scan(response.bytes()), Ok(()));
                let document = serde_json::to_value(kynos::router::Router::<()>::new().mount(kynos::routes![echo]).openapi().expect("describes")).expect("JSON");
                let content = &document["paths"]["/records"]["post"]["requestBody"]["content"];
                assert!(content.get("application/cbor").is_some(), "{content}");
                client.assert_conformance();
            }

            #[tokio::test]
            async fn kynos_refuses_another_media_type_before_the_codec() {
                let response = client().post("/records").body("application/json", b"{}".to_vec()).send().await;
                response.assert_status(StatusCode::UNSUPPORTED_MEDIA_TYPE);
            }

            #[tokio::test]
            async fn kynos_refuses_a_body_over_the_limit_before_the_codec() {
                let body = vec![0x00; usize::try_from(DEFAULT_LIMIT).expect("fits") + 1];
                let (status, detail) = post(&client(), body).await;
                assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{detail:?}");
            }

            #[tokio::test]
            async fn kynos_refuses_a_body_under_a_content_coding_before_the_codec() {
                let body = serde_cbor::to_vec(&record()).expect("encodes");
                let response = client()
                    .post("/records")
                    .header("content-encoding", "gzip")
                    .body("application/cbor", body)
                    .send()
                    .await;
                response.assert_status(StatusCode::UNSUPPORTED_MEDIA_TYPE);
            }

            #[tokio::test]
            async fn a_bound_the_schema_declares_is_kynos_422_after_the_codec() {
                let client = client();
                let within = serde_cbor::to_vec(&Bounded { count: 10 }).expect("encodes");
                let (status, detail) = post_to(&client, "/bounded", within).await;
                assert_eq!(status, StatusCode::NO_CONTENT, "{detail:?}");
                let over = serde_cbor::to_vec(&Bounded { count: 11 }).expect("encodes");
                let (status, detail) = post_to(&client, "/bounded", over).await;
                assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{detail:?}");
                let detail = String::from_utf8_lossy(&detail);
                assert!(detail.contains("/count"), "the member is named: {detail}");
            }

            #[tokio::test]
            async fn each_scan_rule_is_answered_through_kynos() {
                let client = client();
                for raw in [
                    // An indefinite integer, a chunk of another type, a
                    // two-byte simple value below 32, a tag with no content,
                    // a key's code point split across chunks.
                    "1f",
                    "7f 01 ff",
                    "f8 18",
                    "9f c0 ff",
                    "bf c0 ff",
                    "a1 7f 61 c3 61 a9 ff 01",
                    // Ill-formed bytes after a data-model refusal: syntax
                    // wins.
                    "a1 1f 00",
                    "82 40 1c",
                ] {
                    let (status, detail) = post(&client, hex(raw)).await;
                    assert_eq!(status, StatusCode::BAD_REQUEST, "{raw}: {detail:?}");
                }
                // Simple values other than false, true, null and undefined.
                for raw in ["f0", "f3", "f8 20", "f8 ff"] {
                    let (status, detail) = post(&client, record_with("parent", raw)).await;
                    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{raw}: {detail:?}");
                }
            }

            #[tokio::test]
            async fn a_struct_is_not_read_from_an_array() {
                let client = client();
                let map = serde_cbor::to_vec(&Pair { id: 7, name: "seven".into() }).expect("encodes");
                let (status, detail) = post_to(&client, "/pairs", map).await;
                assert_eq!(status, StatusCode::NO_CONTENT, "{detail:?}");
                let array = serde_cbor::to_vec(&(7u64, "seven")).expect("encodes");
                assert_eq!(array, hex("82 07 65 736576656e"));
                let (status, detail) = post_to(&client, "/pairs", array).await;
                assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{detail:?}");
                // `Json<T>` refuses the same array the same way.
                let json = client.post("/pairs.json").json(&serde_json::json!([7, "seven"])).send().await;
                json.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
            }

            #[tokio::test]
            async fn formatted_strings_round_trip_as_the_schema_describes() {
                let client = client();
                let json = formatted_json();
                let body = serde_cbor::to_vec(&json).expect("encodes");
                let (status, detail) = post_to(&client, "/formatted", body).await;
                assert_eq!(status, StatusCode::OK, "{detail:?}");
                assert_eq!(faithful::scan(&detail), Ok(()));
                // Every member is written back as the text `Json<T>` writes.
                let back: serde_json::Value = serde_cbor::from_slice(&detail).expect("CBOR");
                let value: Formatted = serde_json::from_value(json).expect("JSON");
                assert_eq!(back, serde_json::to_value(&value).expect("JSON"));
                let members = back.as_object().expect("a map");
                assert!(members.values().all(serde_json::Value::is_string), "{back}");
                // The description says so.
                let router = kynos::router::Router::<()>::new().mount(kynos::routes![formatted]);
                let document = serde_json::to_value(router.openapi().expect("describes")).expect("JSON");
                let properties = &document["components"]["schemas"]["Formatted"]["properties"];
                assert_eq!(properties["id"]["format"], "uuid", "{properties}");
                assert_eq!(properties["v4"]["format"], "ipv4", "{properties}");
                assert_eq!(properties["at"]["format"], "date-time", "{properties}");
            }

            #[tokio::test]
            async fn nesting_is_refused_where_json_refuses_it() {
                let client = client();
                for (levels, status) in [(127, StatusCode::NO_CONTENT), (128, StatusCode::BAD_REQUEST)] {
                    let json = client.post("/nested.json").body("application/json", nested_json(levels)).send().await;
                    assert_eq!(json.status(), status, "JSON at {levels}: {:?}", json.bytes());
                    let (cbor, detail) = post_to(&client, "/nested", nested_cbor(levels)).await;
                    assert_eq!(cbor, status, "CBOR at {levels}: {detail:?}");
                }
            }

            #[tokio::test]
            async fn ill_formed_bodies_are_400() {
                let client = client();
                let good = serde_cbor::to_vec(&record()).expect("encodes");
                let mut trailing = good.clone();
                trailing.push(0x00);
                for body in [
                    good[..good.len() - 1].to_vec(),
                    trailing,
                    hex("1c"),
                    hex("ff"),
                    hex("a1 61 61 61 ff"),
                    std::iter::repeat_n(0x81, 200).chain([0x00]).collect(),
                    hex("bb ffffffffffffffff"),
                ] {
                    let (status, detail) = post(&client, body.clone()).await;
                    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:02x?}: {detail:?}");
                }
            }

            #[tokio::test]
            async fn values_without_a_json_equivalent_are_422() {
                let client = client();
                for (member, raw) in [
                    ("name", "43 736576"),
                    ("name", "c0 63 736576"),
                    ("id", "c2 41 07"),
                    ("parent", "f7"),
                    ("score", "f9 7e00"),
                    ("score", "f9 7c00"),
                    ("labels", "a1 01 01"),
                ] {
                    let (status, detail) = post(&client, record_with(member, raw)).await;
                    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{member} {raw}: {detail:?}");
                }
                // A repeated key, written by hand: no serializer writes one.
                let repeated = hex("a7 62 6964 07 62 6964 07 64 6e616d65 61 6e 64 74616773 80 65 73636f7265 00 66 706172656e74 f6 66 6c6162656c73 a0");
                let (status, detail) = post(&client, repeated).await;
                assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{detail:?}");
            }

            #[tokio::test]
            async fn a_wrong_shape_is_the_librarys_422() {
                let (status, detail) = post(&client(), record_with("id", "63 736576")).await;
                assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{detail:?}");
            }

            #[tokio::test]
            async fn a_response_the_data_model_cannot_hold_is_500() {
                client().get("/nan").send().await.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
            }

            #[tokio::test]
            async fn accept_picks_the_representation() {
                let client = client();
                let cbor = client.get("/records").header("accept", "application/cbor").send().await;
                cbor.assert_status(StatusCode::OK).assert_header("content-type", "application/cbor");
                let back: Record = serde_cbor::from_slice(cbor.bytes()).expect("CBOR");
                assert_eq!(back, record());
                let json = client.get("/records").header("accept", "application/json").send().await;
                json.assert_header("content-type", "application/json");
                assert_eq!(json.json::<Record>(), record());
            }
        }
    )*};
}

suite! {
    "ciborium" => ciborium: codecs::backend::ciborium::Ciborium,
    "minicbor-serde" => minicbor_serde: codecs::backend::minicbor_serde::MinicborSerde,
    "cbor4ii" => cbor4ii: codecs::backend::cbor4ii::Cbor4ii,
    "cbor2" => cbor2: codecs::backend::cbor2::Cbor2,
    "c2pa_cbor" => c2pa_cbor: codecs::backend::c2pa_cbor::C2paCbor,
}
