//! The Kynos body types the fixture declares, copied from `payloads::kynos`
//! with the same fields, order, serde attributes and native derives.
//!
//! A copy rather than a dependency: `payloads` derives every workload's
//! types, and building them all would add the same harness compile time to
//! every set's clean build, diluting the ratios `xtask compile-time` reports.
//! `tests/body.rs` holds the copies to the originals' encoding.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

codecs::native_type!(Item, Catalog, Echo);

/// One record: the element of [`Catalog`].
#[derive(Serialize, Deserialize, prost::Message)]
#[cfg_attr(feature = "minicbor", derive(minicbor::Encode, minicbor::Decode))]
pub struct Item {
    /// Stable identifier.
    #[prost(uint64, tag = "1")]
    #[cfg_attr(feature = "minicbor", n(0))]
    pub id: u64,
    /// Display name.
    #[prost(string, tag = "2")]
    #[cfg_attr(feature = "minicbor", n(1))]
    pub name: String,
    /// Labels.
    #[prost(string, repeated, tag = "3")]
    #[cfg_attr(feature = "minicbor", n(2))]
    pub tags: Vec<String>,
    /// Liveness.
    #[prost(bool, tag = "4")]
    #[cfg_attr(feature = "minicbor", n(3))]
    pub active: bool,
    /// A two-decimal weight.
    #[prost(double, tag = "5")]
    #[cfg_attr(feature = "minicbor", n(4))]
    pub score: f64,
}

/// [`Item`], borrowing.
#[derive(Debug, Serialize, Deserialize)]
pub struct ItemRef<'a> {
    /// Stable identifier.
    pub id: u64,
    /// Display name.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// Labels.
    #[serde(borrow)]
    pub tags: Vec<Cow<'a, str>>,
    /// Liveness.
    pub active: bool,
    /// A two-decimal weight.
    pub score: f64,
}

/// The `json-large` body.
#[derive(Serialize, Deserialize, prost::Message)]
#[cfg_attr(feature = "minicbor", derive(minicbor::Encode, minicbor::Decode))]
pub struct Catalog {
    /// Fixture revision.
    #[prost(uint32, tag = "1")]
    #[cfg_attr(feature = "minicbor", n(0))]
    pub revision: u32,
    /// Total records available.
    #[prost(uint64, tag = "2")]
    #[cfg_attr(feature = "minicbor", n(1))]
    pub total: u64,
    /// The records.
    #[prost(message, repeated, tag = "3")]
    #[cfg_attr(feature = "minicbor", n(2))]
    pub items: Vec<Item>,
}

/// [`Catalog`], borrowing.
#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogRef<'a> {
    /// Fixture revision.
    pub revision: u32,
    /// Total records available.
    pub total: u64,
    /// The records.
    #[serde(borrow)]
    pub items: Vec<ItemRef<'a>>,
}

/// The `echo-post` body, in both directions.
#[derive(Serialize, Deserialize, prost::Message)]
#[cfg_attr(feature = "minicbor", derive(minicbor::Encode, minicbor::Decode))]
pub struct Echo {
    /// Correlation id.
    #[prost(uint64, tag = "1")]
    #[cfg_attr(feature = "minicbor", n(0))]
    pub id: u64,
    /// The bulk of the body.
    #[prost(string, tag = "2")]
    #[cfg_attr(feature = "minicbor", n(1))]
    pub payload: String,
    /// Checksum of the payload bytes.
    #[prost(uint64, tag = "3")]
    #[cfg_attr(feature = "minicbor", n(2))]
    pub checksum: u64,
}

/// [`Echo`], borrowing.
#[derive(Debug, Serialize, Deserialize)]
pub struct EchoRef<'a> {
    /// Correlation id.
    pub id: u64,
    /// The bulk of the body.
    #[serde(borrow)]
    pub payload: Cow<'a, str>,
    /// Checksum of the payload bytes.
    pub checksum: u64,
}
