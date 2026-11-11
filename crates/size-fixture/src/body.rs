//! The Kynos body types the fixture declares, copied from `payloads::kynos`
//! with the same fields, order, serde attributes and native derives.
//!
//! A copy rather than a dependency: `payloads` derives every workload's
//! types, and building them all would add the same harness compile time to
//! every set's clean build, diluting the ratios `xtask compile-time` reports.
//! `tests/body.rs` holds the copies to the originals' encoding.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// One record: the element of [`Catalog`].
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "prost", derive(prost::Message))]
#[cfg_attr(feature = "minicbor", derive(minicbor::Encode, minicbor::Decode))]
pub struct Item {
    /// Stable identifier.
    #[cfg_attr(feature = "prost", prost(uint64, tag = "1"))]
    #[cfg_attr(feature = "minicbor", n(0))]
    pub id: u64,
    /// Display name.
    #[cfg_attr(feature = "prost", prost(string, tag = "2"))]
    #[cfg_attr(feature = "minicbor", n(1))]
    pub name: String,
    /// Labels.
    #[cfg_attr(feature = "prost", prost(string, repeated, tag = "3"))]
    #[cfg_attr(feature = "minicbor", n(2))]
    pub tags: Vec<String>,
    /// Liveness.
    #[cfg_attr(feature = "prost", prost(bool, tag = "4"))]
    #[cfg_attr(feature = "minicbor", n(3))]
    pub active: bool,
    /// A two-decimal weight.
    #[cfg_attr(feature = "prost", prost(double, tag = "5"))]
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
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "prost", derive(prost::Message))]
#[cfg_attr(feature = "minicbor", derive(minicbor::Encode, minicbor::Decode))]
pub struct Catalog {
    /// Fixture revision.
    #[cfg_attr(feature = "prost", prost(uint32, tag = "1"))]
    #[cfg_attr(feature = "minicbor", n(0))]
    pub revision: u32,
    /// Total records available.
    #[cfg_attr(feature = "prost", prost(uint64, tag = "2"))]
    #[cfg_attr(feature = "minicbor", n(1))]
    pub total: u64,
    /// The records.
    #[cfg_attr(feature = "prost", prost(message, repeated, tag = "3"))]
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
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "prost", derive(prost::Message))]
#[cfg_attr(feature = "minicbor", derive(minicbor::Encode, minicbor::Decode))]
pub struct Echo {
    /// Correlation id.
    #[cfg_attr(feature = "prost", prost(uint64, tag = "1"))]
    #[cfg_attr(feature = "minicbor", n(0))]
    pub id: u64,
    /// The bulk of the body.
    #[cfg_attr(feature = "prost", prost(string, tag = "2"))]
    #[cfg_attr(feature = "minicbor", n(1))]
    pub payload: String,
    /// Checksum of the payload bytes.
    #[cfg_attr(feature = "prost", prost(uint64, tag = "3"))]
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
