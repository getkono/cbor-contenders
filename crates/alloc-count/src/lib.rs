//! A global allocator that counts, around the system allocator.
//!
//! Install it with `#[global_allocator]` in a binary that measures, never in a
//! library: a global allocator is process-wide.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// The counting allocator.
#[derive(Debug)]
pub struct Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static REALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE: AtomicU64 = AtomicU64::new(0);
static PEAK: AtomicU64 = AtomicU64::new(0);

fn grow(by: u64) {
    let live = LIVE.fetch_add(by, Relaxed) + by;
    PEAK.fetch_max(live, Relaxed);
}

/// Count a fresh block of `size` bytes, if the system allocator gave one.
fn allocated(ptr: *mut u8, size: usize) -> *mut u8 {
    if !ptr.is_null() {
        ALLOCATIONS.fetch_add(1, Relaxed);
        BYTES.fetch_add(size as u64, Relaxed);
        grow(size as u64);
    }
    ptr
}

// SAFETY: every method forwards to `System` with the caller's arguments
// unchanged; the counters are side effects that allocate nothing.
//
// Only what succeeds is counted: a null return allocated nothing, and a
// failed `realloc` leaves the old block as it was, so a caller that handles
// the failure (`try_reserve`) is charged no block it never had.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged.
        allocated(unsafe { System.alloc(layout) }, layout.size())
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged.
        allocated(unsafe { System.alloc_zeroed(layout) }, layout.size())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as u64, Relaxed);
        // SAFETY: forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded unchanged.
        let moved = unsafe { System.realloc(ptr, layout, new_size) };
        if moved.is_null() {
            return moved;
        }
        REALLOCATIONS.fetch_add(1, Relaxed);
        if new_size > layout.size() {
            let by = (new_size - layout.size()) as u64;
            BYTES.fetch_add(by, Relaxed);
            grow(by);
        } else {
            LIVE.fetch_sub((layout.size() - new_size) as u64, Relaxed);
        }
        moved
    }
}

/// Counters at one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Snapshot {
    /// Fresh blocks (`alloc`, `alloc_zeroed`).
    pub allocations: u64,
    /// Resizes of an existing block.
    pub reallocations: u64,
    /// Bytes requested, counting only the growth of a reallocation.
    pub bytes: u64,
    /// Bytes live now.
    pub live: u64,
}

impl Snapshot {
    /// Read the counters.
    #[must_use]
    pub fn now() -> Self {
        Self {
            allocations: ALLOCATIONS.load(Relaxed),
            reallocations: REALLOCATIONS.load(Relaxed),
            bytes: BYTES.load(Relaxed),
            live: LIVE.load(Relaxed),
        }
    }
}

/// Reset the peak to the current live size, so the next [`peak`] reports the
/// high-water mark of what runs after this call.
pub fn reset_peak() {
    PEAK.store(LIVE.load(Relaxed), Relaxed);
}

/// The high-water mark of live bytes since the last [`reset_peak`].
#[must_use]
pub fn peak() -> u64 {
    PEAK.load(Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request the system cannot satisfy returns null, and nothing is
    /// counted for it. The allocator is called directly, not installed, so
    /// only these calls move the counters.
    #[test]
    fn a_failed_allocation_counts_nothing() {
        // Just under `isize::MAX` bytes at page alignment: a valid layout no
        // system can map.
        let impossible = Layout::from_size_align(isize::MAX as usize - 4095, 4096).expect("a valid layout");
        // Page-aligned, so `System.realloc` moves it through a fresh aligned
        // allocation, which fails as the one above does.
        let small = Layout::from_size_align(16, 4096).expect("a valid layout");
        let before = Snapshot::now();
        // SAFETY: a non-zero size; the result is only compared with null.
        assert!(unsafe { Counting.alloc(impossible) }.is_null());
        // SAFETY: as above.
        assert!(unsafe { Counting.alloc_zeroed(impossible) }.is_null());
        assert_eq!(Snapshot::now(), before);
        assert!(peak() < impossible.size() as u64);

        // SAFETY: a non-zero size; freed below with the same layout.
        let block = unsafe { Counting.alloc(small) };
        assert!(!block.is_null());
        let allocated = Snapshot::now();
        assert_eq!(allocated.allocations - before.allocations, 1);
        assert_eq!(allocated.bytes - before.bytes, 16);
        // SAFETY: `block` is live with `small`'s layout, and the new size is
        // non-zero and does not overflow `isize` when rounded to `small`'s
        // alignment.
        let grown = unsafe { Counting.realloc(block, small, impossible.size()) };
        assert!(grown.is_null(), "the block stays where it was");
        assert_eq!(Snapshot::now(), allocated);
        assert!(peak() < impossible.size() as u64);
        // SAFETY: the failed `realloc` left `block` live with `small`'s layout.
        unsafe { Counting.dealloc(block, small) };
    }
}
