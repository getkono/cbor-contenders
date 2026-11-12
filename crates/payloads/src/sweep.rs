//! The size sweep: the same record shape at eight body sizes.
//!
//! Each body is exactly `N` bytes of CBOR (serde_cbor's encoding, the input
//! every CBOR decoder reads): as many whole records as fit, with the first
//! record's name and tags rewritten to make up the rest, which is less than
//! one more record. The same value is larger in JSON and smaller in
//! protobuf; the sweep is sized for the format under comparison.

use crate::kynos::{Item, ItemRef, item};
use crate::{Group, Workload, cbor};

/// A `Vec<Item>` encoding to exactly `N` bytes of CBOR.
#[derive(Debug)]
pub struct Sweep<const N: usize>;

/// Bytes of a CBOR head with argument `n`.
fn head_len(n: usize) -> usize {
    match n {
        0..=23 => 1,
        24..=0xff => 2,
        0x100..=0xffff => 3,
        0x1_0000..=0xffff_ffff => 5,
        _ => 9,
    }
}

/// The length of a text string that encodes to exactly `extra` bytes more
/// than an empty one, if one does. None does for the few values a widening
/// head steps over: 23 and 24, 257, 65 538 and 65 539.
fn text_adding(extra: usize) -> Option<usize> {
    [1, 2, 3, 5, 9]
        .into_iter()
        .filter_map(|head| (extra + 1).checked_sub(head))
        .find(|&len| head_len(len) + len == extra + 1)
}

/// The records of a sweep body of `target` bytes.
///
/// Every record but the first is [`item`] as it is. The count is the most
/// records whose encoding, with the first one's name and tags emptied, fits
/// the target, so the remainder is less than the next record (and the
/// array head's growth) would have been. The first record's name and one
/// tag make up that remainder. A name cannot grow by every byte count (its
/// head widens at 24, 256 and 65 536 bytes, skipping at most two adjacent
/// counts), so the tag takes 0 to 23 bytes of it, and of 24 consecutive
/// counts left for the name at least one is a length's: the construction
/// is exact for every target from the floor up.
///
/// # Panics
/// If `target` is below the smallest body one record can make.
#[must_use]
pub fn records(target: usize) -> Vec<Item> {
    let mut first = item(0);
    first.name.clear();
    first.tags.clear();
    // The records' encodings without the array head: whole records are
    // added while the array, head included, still fits.
    let mut sum = cbor(&first).len();
    let mut count = 1;
    loop {
        let next = cbor(&item(count as u64)).len();
        if head_len(count + 1) + sum + next > target {
            break;
        }
        sum += next;
        count += 1;
    }
    let base = head_len(count) + sum;
    assert!(base <= target, "sweep target {target} is below the {base}-byte floor");
    let remainder = target - base;
    // A tag of `tag - 1` bytes adds `tag` bytes; none adds none.
    let split = (0..=23.min(remainder)).find_map(|tag| text_adding(remainder - tag).map(|name| (name, tag)));
    let Some((name, tag)) = split else {
        panic!("no name and tag make up {remainder} bytes: the gaps are wider than this construction assumes")
    };
    first.name = "x".repeat(name);
    if tag > 0 {
        first.tags.push("y".repeat(tag - 1));
    }
    let items: Vec<Item> = std::iter::once(first).chain((1..count as u64).map(item)).collect();
    assert_eq!(cbor(&items).len(), target, "the sweep construction is not exact");
    items
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
