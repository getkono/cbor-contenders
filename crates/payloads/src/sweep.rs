//! The size sweep: the same record shape at eight body sizes.
//!
//! Each body is exactly `N` bytes of CBOR (serde_cbor's encoding, the input
//! every CBOR decoder reads): `N / 100` records (at least one), with the
//! first record's tags dropped and its name padded to make up the
//! difference. The same value is larger in JSON and smaller in protobuf; the
//! sweep is sized for the format under comparison.

use crate::kynos::{Item, ItemRef, item};
use crate::{Group, Workload, cbor};

/// A `Vec<Item>` encoding to exactly `N` bytes of CBOR.
#[derive(Debug)]
pub struct Sweep<const N: usize>;

/// The records of a sweep body of `target` bytes.
///
/// # Panics
/// If `target` is below the smallest body one record can make.
#[must_use]
pub fn records(target: usize) -> Vec<Item> {
    let count = (target / 100).max(1) as u64;
    let mut items: Vec<Item> = (0..count).map(item).collect();
    items[0].name.clear();
    items[0].tags.clear();
    let base = cbor(&items).len();
    assert!(base <= target, "sweep target {target} is below the {base}-byte floor");
    // A text string's head grows by a byte at lengths 24, 256 and 65 536, so
    // the padding is found by trying the lengths that land on the target.
    let mut pad = target - base;
    loop {
        items[0].name = "x".repeat(pad);
        let size = cbor(&items).len();
        match size.cmp(&target) {
            std::cmp::Ordering::Equal => return items,
            std::cmp::Ordering::Greater if pad > 0 => pad -= 1,
            _ => unreachable!("no padding of record 0 makes {target} bytes"),
        }
    }
}

/// The sweep's records as one protobuf message: a protobuf message cannot be
/// a bare list, so the list is field 1.
#[derive(Clone, PartialEq, prost::Message)]
#[cfg_attr(feature = "minicbor", derive(minicbor::Encode, minicbor::Decode))]
pub struct Items {
    /// The records.
    #[prost(message, repeated, tag = "1")]
    #[cfg_attr(feature = "minicbor", n(0))]
    pub items: Vec<Item>,
}

codecs::native_type!(Items);

impl<const N: usize> Workload for Sweep<N> {
    const NAME: &'static str = match N {
        64 => "sweep-64b",
        256 => "sweep-256b",
        1024 => "sweep-1kib",
        4096 => "sweep-4kib",
        16384 => "sweep-16kib",
        65536 => "sweep-64kib",
        262_144 => "sweep-256kib",
        1_048_576 => "sweep-1mib",
        _ => panic!("unnamed sweep size"),
    };
    const GROUP: Group = Group::Sweep;
    type Owned = Vec<Item>;
    type Borrowed<'de> = Vec<ItemRef<'de>>;
    type Native = Items;

    fn value() -> Vec<Item> {
        records(N)
    }

    fn native() -> Option<Items> {
        Some(Items { items: records(N) })
    }
}
