//! Every entry answers every route correctly through Kynos, the floors serve
//! the reference bytes, and the server's body copies are the originals.

use bytes::Bytes;
use codecs::Format;
use e2e::Route;
use e2e::loopback::{Loopback, request, verify};
use http::StatusCode;
use payloads::Workload;
use payloads::kynos::{EchoPost, JsonLarge, JsonSmall};

/// The baselines and floors answer every route correctly; a candidate is
/// served and judged (a wrong answer is its result, which `xtask e2e-count`
/// records, not a harness failure).
#[tokio::test]
async fn every_entry_is_served_and_the_references_answer_correctly() {
    for entry in e2e::entry::compiled() {
        for &route in Route::ALL {
            let answer = verify(entry, route)
                .await
                .unwrap_or_else(|why| panic!("{entry}: {why}"));
            if ["serde_json", "prost"].contains(&entry) || entry.starts_with("floor") {
                answer.unwrap_or_else(|why| panic!("{entry}: {why}"));
            }
        }
    }
}

async fn send(entry: &str, route: Route, body: Vec<u8>) -> StatusCode {
    let format = e2e::entry::format(entry).expect("compiled");
    let service = e2e::entry::service(entry).expect("compiled").expect("builds");
    let mut loopback = Loopback::connect(format, service).await.expect("connects");
    loopback
        .send(request(format, route, &Bytes::from(body)))
        .await
        .expect("served")
        .0
}

#[tokio::test]
async fn bad_bodies_get_the_statuses_kynos_draws() {
    let mut truncated = EchoPost::bytes(Format::Json);
    truncated.truncate(10);
    assert_eq!(
        send("serde_json", Route::EchoPost, truncated).await,
        StatusCode::BAD_REQUEST
    );
    let shape = payloads::reject::RejectShape::bytes(Format::Json);
    assert_eq!(
        send("serde_json", Route::JsonLargePost, shape).await,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    for entry in e2e::entry::compiled() {
        if e2e::entry::format(entry) != Some(Format::Cbor) || entry.starts_with("floor") {
            continue;
        }
        let mut truncated = EchoPost::bytes(Format::Cbor);
        truncated.truncate(10);
        assert_eq!(
            send(entry, Route::EchoPost, truncated).await,
            StatusCode::BAD_REQUEST,
            "{entry}"
        );
        let shape = payloads::reject::RejectShape::bytes(Format::Cbor);
        // Through the worked codec every candidate answers this 422: the
        // scan passes it and the library refuses the shape (or, for one that
        // cannot read the corpus's half-width floats, those).
        assert_eq!(
            send(entry, Route::JsonLargePost, shape).await,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{entry}"
        );
    }
}

/// The copies read and write exactly what the originals do.
#[test]
fn the_body_copies_are_the_originals() {
    fn check<W: Workload, T: serde::Serialize + serde::de::DeserializeOwned + prost::Message + Default>() {
        let json = W::bytes(Format::Json);
        let back: T = serde_json::from_slice(&json).expect("JSON");
        assert_eq!(serde_json::to_vec(&back).expect("JSON"), json, "{}", W::NAME);
        let cbor = W::bytes(Format::Cbor);
        let back: T = serde_cbor::from_slice(&cbor).expect("CBOR");
        assert_eq!(serde_cbor::to_vec(&back).expect("CBOR"), cbor, "{}", W::NAME);
        let proto = W::bytes(Format::Protobuf);
        assert_eq!(
            T::decode(&proto[..]).expect("protobuf").encode_to_vec(),
            proto,
            "{}",
            W::NAME
        );
    }
    check::<JsonSmall, e2e::body::Item>();
    check::<EchoPost, e2e::body::Echo>();
    check::<JsonLarge, e2e::body::Catalog>();
}
