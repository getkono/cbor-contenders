//! `BodyWriter` writes what a `Vec` would, and allocates as kynos 0.5.0's
//! does: one block of exactly the body's length up to the stack's size, and
//! a spilled buffer of at least twice the stack past it.

use std::io::Write;
use std::sync::Mutex;

use alloc_count::{Counting, Snapshot};
use codecs::writer::{BodyWriter, STACK_OCTETS};

#[global_allocator]
static COUNTING: Counting = Counting;

/// The counters are process-wide, so no two tests count at once.
static SERIAL: Mutex<()> = Mutex::new(());

/// `total` octets written in pieces of `piece`, the pattern a serializer
/// writes in.
fn body(total: usize) -> Vec<u8> {
    (0..total).map(|i| (i * 31 % 251) as u8).collect()
}

fn write(writer: &mut BodyWriter, octets: &[u8], piece: usize) {
    for chunk in octets.chunks(piece.max(1)) {
        writer.write_all(chunk).expect("infallible");
    }
}

#[test]
fn the_octets_are_what_a_vec_holds() {
    let _serial = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for total in [
        0,
        1,
        STACK_OCTETS - 1,
        STACK_OCTETS,
        STACK_OCTETS + 1,
        2 * STACK_OCTETS + 7,
        100_000,
    ] {
        for piece in [1, 3, 64, STACK_OCTETS, 5000] {
            let octets = body(total);
            let mut writer = BodyWriter::new();
            write(&mut writer, &octets, piece);
            assert_eq!(&writer.finish()[..], &octets[..], "{total} in pieces of {piece}");
        }
    }
    // Through a serializer, as Kynos uses it.
    let value: Vec<String> = (0..300).map(|i| format!("item {i}")).collect();
    let mut writer = BodyWriter::new();
    serde_json::to_writer(&mut writer, &value).expect("serializes");
    assert_eq!(
        &writer.finish()[..],
        &serde_json::to_vec(&value).expect("serializes")[..]
    );
}

#[test]
fn a_body_that_fits_the_stack_is_one_exact_allocation() {
    let _serial = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for total in [1, 100, STACK_OCTETS] {
        let octets = body(total);
        let mut writer = BodyWriter::new();
        let before = Snapshot::now();
        write(&mut writer, &octets, 7);
        assert_eq!(
            Snapshot::now().allocations,
            before.allocations,
            "{total}: the stack is written"
        );
        let bytes = writer.finish();
        let after = Snapshot::now();
        assert_eq!(after.allocations - before.allocations, 1, "{total}");
        assert_eq!(after.reallocations, before.reallocations, "{total}");
        assert_eq!(after.bytes - before.bytes, total as u64, "{total}");
        drop(bytes);
    }
    // An empty body allocates nothing.
    let before = Snapshot::now();
    let bytes = BodyWriter::new().finish();
    assert!(bytes.is_empty());
    assert_eq!(Snapshot::now().allocations, before.allocations);
}

#[test]
fn a_larger_body_spills_to_at_least_twice_the_stack() {
    let _serial = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    // Crossing the stack with a small piece: the spill is exactly 2 KiB.
    let octets = body(STACK_OCTETS + 1);
    let mut writer = BodyWriter::new();
    let before = Snapshot::now();
    write(&mut writer, &octets, 1);
    let after = Snapshot::now();
    assert_eq!(after.allocations - before.allocations, 1);
    assert_eq!(after.bytes - before.bytes, 2 * STACK_OCTETS as u64);
    assert_eq!(after.reallocations, before.reallocations);
    // Filling the spill does not grow it.
    let rest = body(2 * STACK_OCTETS - octets.len());
    write(&mut writer, &rest, 1);
    assert_eq!(Snapshot::now().reallocations, before.reallocations);
    assert_eq!(writer.finish().len(), 2 * STACK_OCTETS);

    // One piece larger than both: the spill is sized to hold it.
    let big = body(3 * STACK_OCTETS);
    let mut writer = BodyWriter::new();
    write(&mut writer, &big[..10], 10);
    let before = Snapshot::now();
    write(&mut writer, &big[10..], big.len());
    let after = Snapshot::now();
    assert_eq!(after.allocations - before.allocations, 1);
    assert_eq!(after.bytes - before.bytes, big.len() as u64);
    assert_eq!(&writer.finish()[..], &big[..]);
}
