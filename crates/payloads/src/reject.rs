//! Bodies a server must refuse: what turning a bad request away costs.
//!
//! Both are `json-large` damaged in one place, decoded into its types, so
//! the work before the damage is the work a valid body costs and the
//! difference is where the decoder stops. Their bytes encode no value, so
//! they have no encode: [`Workload::value`] is the value they were damaged
//! from.

use codecs::Format;
use serde::Serialize;

use crate::kynos::{CATALOG_ITEMS, Catalog, CatalogRef, Item, JsonLarge};
use crate::{Expect, Group, NotCarried, Workload, encode};

/// `json-large`'s bytes cut at nine tenths of their length: the body ends
/// partway through the items, so the decoder reads 90 % of a valid body
/// before it can refuse. End of input, 400. Carried by the native formats
/// too: a protobuf message cut inside a field is refused like any other.
#[derive(Debug)]
pub struct RejectTruncated;

impl Workload for RejectTruncated {
    const NAME: &'static str = "reject-truncated";
    const GROUP: Group = Group::Family;
    const EXPECT: Expect = Expect::Reject(400);
    type Owned = Catalog;
    type Borrowed<'de> = CatalogRef<'de>;
    type Native = Catalog;

    fn value() -> Catalog {
        JsonLarge::value()
    }

    fn native() -> Option<Catalog> {
        Some(Self::value())
    }

    fn bytes(format: Format) -> Vec<u8> {
        let mut bytes = JsonLarge::bytes(format);
        bytes.truncate(bytes.len() * 9 / 10);
        bytes
    }
}

/// An [`Item`] or one whose `id` is a string, for writing damaged bodies.
#[derive(Serialize)]
#[serde(untagged)]
enum Slot<'a> {
    Good(&'a Item),
    Bad {
        id: String,
        name: &'a str,
        tags: &'a [String],
        active: bool,
        score: f64,
    },
}

/// [`Catalog`] with one damaged item.
#[derive(Serialize)]
struct Damaged<'a> {
    revision: u32,
    total: u64,
    items: Vec<Slot<'a>>,
}

/// `json-large`'s bytes with the last item's `id` written as a text string:
/// a valid item of the wrong shape, found after every other item has been
/// decoded. Data, 422. Not carried by the native formats: prost has no
/// shape error (a wrong wire type is refused as malformed).
#[derive(Debug)]
pub struct RejectShape;

impl Workload for RejectShape {
    const NAME: &'static str = "reject-shape";
    const GROUP: Group = Group::Family;
    const EXPECT: Expect = Expect::Reject(422);
    type Owned = Catalog;
    type Borrowed<'de> = CatalogRef<'de>;
    type Native = NotCarried;

    fn value() -> Catalog {
        JsonLarge::value()
    }

    fn bytes(format: Format) -> Vec<u8> {
        if matches!(format, Format::Protobuf | Format::MinicborNative) {
            return Vec::new();
        }
        let catalog = Self::value();
        let last = CATALOG_ITEMS - 1;
        let items = catalog
            .items
            .iter()
            .map(|item| {
                if item.id == last {
                    Slot::Bad {
                        id: last.to_string(),
                        name: &item.name,
                        tags: &item.tags,
                        active: item.active,
                        score: item.score,
                    }
                } else {
                    Slot::Good(item)
                }
            })
            .collect();
        encode(
            format,
            &Damaged {
                revision: catalog.revision,
                total: catalog.total,
                items,
            },
        )
    }
}
