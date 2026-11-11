//! The Kynos body types the server declares, copied from `payloads::kynos`
//! with the same fields and order, and deriving `Schema` as a Kynos
//! application's body types do. `tests/entries.rs` holds them to the
//! originals' encoding in every format.

use std::sync::Arc;

use kynos::Schema;
use serde::{Deserialize, Serialize};

/// One record: the `json-small` body and the element of `json-large`.
#[derive(Clone, PartialEq, Schema, Serialize, Deserialize, prost::Message)]
pub struct Item {
    /// Stable identifier.
    #[prost(uint64, tag = "1")]
    pub id: u64,
    /// Display name.
    #[prost(string, tag = "2")]
    pub name: String,
    /// Labels.
    #[prost(string, repeated, tag = "3")]
    pub tags: Vec<String>,
    /// Liveness.
    #[prost(bool, tag = "4")]
    pub active: bool,
    /// A two-decimal weight.
    #[prost(double, tag = "5")]
    pub score: f64,
}

/// The `json-large` body.
#[derive(Clone, PartialEq, Schema, Serialize, Deserialize, prost::Message)]
pub struct Catalog {
    /// Fixture revision.
    #[prost(uint32, tag = "1")]
    pub revision: u32,
    /// Total records available.
    #[prost(uint64, tag = "2")]
    pub total: u64,
    /// The records.
    #[prost(message, repeated, tag = "3")]
    pub items: Vec<Item>,
}

/// The `echo-post` body, in both directions.
#[derive(Clone, PartialEq, Schema, Serialize, Deserialize, prost::Message)]
pub struct Echo {
    /// Correlation id.
    #[prost(uint64, tag = "1")]
    pub id: u64,
    /// The bulk of the body.
    #[prost(string, tag = "2")]
    pub payload: String,
    /// Checksum of the payload bytes.
    #[prost(uint64, tag = "3")]
    pub checksum: u64,
}

/// A response value built once and shared by every request, so a `GET`
/// handler encodes it without cloning it. Serialize-transparent; for prost,
/// a message that encodes as the one it holds (it is never decoded).
#[derive(Debug, Default, PartialEq)]
pub struct Shared<T>(pub Arc<T>);

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T: Serialize> Serialize for Shared<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Shared<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(|value| Self(Arc::new(value)))
    }
}

impl<T: kynos::schema::Schema> kynos::schema::Schema for Shared<T> {
    fn schema(registry: &mut kynos::schema::registry::Registry) -> kynos::openapi::Schema {
        <Arc<T> as kynos::schema::Schema>::schema(registry)
    }

    fn name() -> Option<kynos::openapi::ComponentName> {
        T::name()
    }
}

impl<T: prost::Message + Default> prost::Message for Shared<T> {
    fn encode_raw(&self, buf: &mut impl prost::bytes::BufMut) {
        self.0.encode_raw(buf);
    }

    fn merge_field(
        &mut self,
        tag: u32,
        wire_type: prost::encoding::WireType,
        buf: &mut impl prost::bytes::Buf,
        ctx: prost::encoding::DecodeContext,
    ) -> Result<(), prost::DecodeError> {
        // Only ever encoded; a decode goes into `T` itself.
        prost::encoding::skip_field(wire_type, tag, buf, ctx)
    }

    fn encoded_len(&self) -> usize {
        self.0.encoded_len()
    }

    fn clear(&mut self) {
        self.0 = Arc::default();
    }
}
