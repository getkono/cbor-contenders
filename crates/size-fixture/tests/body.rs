//! The fixture's copied body types read and write exactly what the
//! `payloads` originals do, in every format.

use codecs::Format;
use payloads::Workload;
use payloads::kynos::{EchoPost, JsonLarge, JsonSmall};
use serde::Serialize;
use serde::de::DeserializeOwned;
use size_fixture::body::{Catalog, Echo, Item};

/// Decode `W`'s bytes into the copy `T` and re-encode them: the same bytes.
fn same_bytes<W: Workload, T: Serialize + DeserializeOwned + Protobuf>() {
    let json = W::bytes(Format::Json);
    let back: T = serde_json::from_slice(&json).expect("JSON");
    assert_eq!(serde_json::to_vec(&back).expect("JSON"), json, "{}", W::NAME);
    let cbor = W::bytes(Format::Cbor);
    let back: T = serde_cbor::from_slice(&cbor).expect("CBOR");
    assert_eq!(payloads::cbor(&back), cbor, "{}", W::NAME);
    T::same_protobuf(&W::bytes(Format::Protobuf), W::NAME);
}

/// The protobuf half of [`same_bytes`], where the body types derive it.
trait Protobuf {
    fn same_protobuf(proto: &[u8], name: &str);
}

#[cfg(feature = "prost")]
impl<T: prost::Message + Default> Protobuf for T {
    fn same_protobuf(proto: &[u8], name: &str) {
        let back = T::decode(proto).expect("protobuf");
        assert_eq!(back.encode_to_vec(), proto, "{name}");
    }
}

#[cfg(not(feature = "prost"))]
impl<T> Protobuf for T {
    fn same_protobuf(_: &[u8], _: &str) {}
}

#[test]
fn owned_copies_match_the_originals() {
    same_bytes::<JsonSmall, Item>();
    same_bytes::<EchoPost, Echo>();
    same_bytes::<JsonLarge, Catalog>();
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
        size_fixture::round_trip(echo_bytes.clone().into(), false),
        [echo.payload.len() as u8]
    );
    assert_eq!(
        size_fixture::round_trip(catalog_bytes.clone().into(), false),
        [catalog.items.len() as u8]
    );
    // Re-encoding writes the same bytes again.
    assert_eq!(size_fixture::round_trip(echo_bytes.clone().into(), true), echo_bytes);
    // A body cut short is refused with a status, not a panic.
    let cut = size_fixture::round_trip(catalog_bytes[..catalog_bytes.len() / 2].to_vec().into(), false);
    assert!(cut.starts_with(b"400 "), "{}", String::from_utf8_lossy(&cut));
}
