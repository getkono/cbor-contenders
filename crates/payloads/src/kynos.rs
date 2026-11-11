//! The three shapes a Kynos server is benchmarked on, ported verbatim.
//!
//! Field order is serialization order and the JSON bytes are pinned by
//! kynos-bench's digests, so these types must not be reordered. The owned
//! types also derive `prost::Message` (the `Protobuf<T>` baseline) and, in the
//! `minicbor` set, minicbor's `Encode`/`Decode`, with tags in field order.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::{Group, Workload};

codecs::native_type!(Item, Catalog, Echo);

/// One record: the `json-small` body and the element of `json-large`.
#[derive(Clone, PartialEq, Serialize, Deserialize, prost::Message)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, PartialEq, Serialize, Deserialize, prost::Message)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, PartialEq, Serialize, Deserialize, prost::Message)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EchoRef<'a> {
    /// Correlation id.
    pub id: u64,
    /// The bulk of the body.
    #[serde(borrow)]
    pub payload: Cow<'a, str>,
    /// Checksum of the payload bytes.
    pub checksum: u64,
}

/// Records in `json-large`; lands the body near 64 KiB.
pub const CATALOG_ITEMS: u64 = 705;
/// Which record `json-small` serves; not zero, so fields do not collapse.
pub const SMALL_ITEM: u64 = 100;
/// `echo-post` payload length.
pub const ECHO_PAYLOAD: usize = 1024;

/// The record at `index`, a closed-form function of the index.
#[must_use]
pub fn item(index: u64) -> Item {
    Item {
        id: index,
        name: format!("item-{index:06}"),
        tags: vec![
            format!("tag-{}", index % 7),
            format!("tag-{}", index % 13),
            format!("tag-{}", index % 29),
        ],
        active: !index.is_multiple_of(3),
        score: f64::from(u32::try_from(index % 10_000).unwrap_or(0)) / 100.0,
    }
}

/// Additive checksum proving the payload was read.
#[must_use]
pub fn checksum(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(u64::from(*b)))
}

/// `json-small`: one [`Item`], 91 bytes of JSON.
#[derive(Debug)]
pub struct JsonSmall;

impl Workload for JsonSmall {
    const NAME: &'static str = "json-small";
    const GROUP: Group = Group::Kynos;
    type Owned = Item;
    type Borrowed<'de> = ItemRef<'de>;
    type Native = Item;

    fn value() -> Item {
        item(SMALL_ITEM)
    }

    fn native() -> Option<Item> {
        Some(Self::value())
    }
}

/// `echo-post`: a 1 KiB [`Echo`], decoded and re-encoded.
#[derive(Debug)]
pub struct EchoPost;

impl Workload for EchoPost {
    const NAME: &'static str = "echo-post";
    const GROUP: Group = Group::Kynos;
    type Owned = Echo;
    type Borrowed<'de> = EchoRef<'de>;
    type Native = Echo;

    fn native() -> Option<Echo> {
        Some(Self::value())
    }

    fn value() -> Echo {
        let payload: String = (0..ECHO_PAYLOAD).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
        let checksum = checksum(payload.as_bytes());
        Echo {
            id: 7,
            payload,
            checksum,
        }
    }
}

/// `json-large`: a [`Catalog`] of 705 items, about 64 KiB of JSON.
#[derive(Debug)]
pub struct JsonLarge;

impl Workload for JsonLarge {
    const NAME: &'static str = "json-large";
    const GROUP: Group = Group::Kynos;
    type Owned = Catalog;
    type Borrowed<'de> = CatalogRef<'de>;
    type Native = Catalog;

    fn native() -> Option<Catalog> {
        Some(Self::value())
    }

    fn value() -> Catalog {
        Catalog {
            revision: 1,
            total: 1_000_000,
            items: (0..CATALOG_ITEMS).map(item).collect(),
        }
    }
}
