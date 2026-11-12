//! The Kynos body types the fixture declares, copied from `payloads::kynos`
//! with the same fields, order, serde attributes and native derives.
//!
//! A copy rather than a dependency: `payloads` derives every workload's
//! types, and building them all would add the same harness compile time to
//! every set's clean build, diluting the ratios `xtask compile-time` reports.
//! `tests/body.rs` holds the copies to the originals' encoding. Only the
//! owned forms are copied: the fixture mounts a codec as Kynos does, and
//! `BodyCodec::decode` produces a `DeserializeOwned` value.

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
