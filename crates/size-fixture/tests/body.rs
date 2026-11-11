//! The fixture's copied body types read and write exactly what the
//! `payloads` originals do, in every format.

use codecs::{Format, NativeType};
use payloads::Workload;
use payloads::kynos::{EchoPost, JsonLarge, JsonSmall};
use serde::Serialize;
use serde::de::DeserializeOwned;
use size_fixture::body::{Catalog, CatalogRef, Echo, EchoRef, Item, ItemRef};

/// Decode `W`'s bytes into the copy `T` and re-encode them: the same bytes.
fn same_bytes<W: Workload, T: Serialize + DeserializeOwned + NativeType>() {
    let json = W::bytes(Format::Json);
    let back: T = serde_json::from_slice(&json).expect("JSON");
    assert_eq!(serde_json::to_vec(&back).expect("JSON"), json, "{}", W::NAME);
    let cbor = W::bytes(Format::Cbor);
    let back: T = serde_cbor::from_slice(&cbor).expect("CBOR");
    assert_eq!(payloads::cbor(&back), cbor, "{}", W::NAME);
    let proto = W::bytes(Format::Protobuf);
    let back = T::proto_decode(proto.clone().into()).expect("protobuf");
    let mut again = Vec::new();
    back.proto_encode(&mut again).expect("protobuf");
    assert_eq!(again, proto, "{}", W::NAME);
}

#[test]
fn owned_copies_match_the_originals() {
    same_bytes::<JsonSmall, Item>();
    same_bytes::<EchoPost, Echo>();
    same_bytes::<JsonLarge, Catalog>();
}

#[test]
fn borrowed_copies_match_the_originals() {
    let small = JsonSmall::bytes(Format::Json);
    let back: ItemRef<'_> = serde_json::from_slice(&small).expect("JSON");
    assert_eq!(serde_json::to_vec(&back).expect("JSON"), small);
    let echo = EchoPost::bytes(Format::Json);
    let back: EchoRef<'_> = serde_json::from_slice(&echo).expect("JSON");
    assert_eq!(serde_json::to_vec(&back).expect("JSON"), echo);
    let large = JsonLarge::bytes(Format::Json);
    let back: CatalogRef<'_> = serde_json::from_slice(&large).expect("JSON");
    assert_eq!(serde_json::to_vec(&back).expect("JSON"), large);
}

/// The fixture dispatches each body to its type, through the selected
/// codec's own encoding of it (so a codec's refusal of another library's
/// encoding is conformance's finding, not this test's).
#[test]
fn the_fixture_decodes_both_bodies() {
    use size_fixture::{Codec, Selected};
    let echo: Echo = serde_json::from_slice(&EchoPost::bytes(Format::Json)).expect("JSON");
    let catalog: Catalog = serde_json::from_slice(&JsonLarge::bytes(Format::Json)).expect("JSON");
    let echo_bytes = Selected::encode_echo(&echo).expect("encodes");
    let catalog_bytes = Selected::encode_catalog(&catalog).expect("encodes");
    assert_eq!(
        size_fixture::round_trip(echo_bytes.into(), false),
        [echo.payload.len() as u8]
    );
    assert_eq!(
        size_fixture::round_trip(catalog_bytes.into(), false),
        [catalog.items.len() as u8]
    );
}
