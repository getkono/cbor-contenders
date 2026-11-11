//! The buffer Kynos encodes a response body into.
//!
//! A copy of kynos 0.5.0's `BodyWriter`, from the crate this workspace pins
//! (`kynos-0.5.0/src/response/codec/mod.rs`, published from getkono/kynos
//! commit `39f787d76b7f1ad4336193ea9224b6d7b8d7b445`, tag `kynos-v0.5.0`).
//! Kynos hands it to `BodyCodec::encode` for every `Encoded<C, T>` response
//! and serializes every `Json<T>` into it with `serde_json::to_writer`, so an
//! encode measured through it pays what a Kynos server pays: a body of at
//! most 1 KiB is written on the stack and copied once into an allocation of
//! exactly its length; a larger one spills to a heap buffer of at least
//! 2 KiB, which grows as a `Vec` does and becomes the body's `Bytes`.
//!
//! The one departure is the receiver of [`BodyWriter::finish`]: kynos 0.5.0
//! names it `into_bytes(self)`; Kynos's `master` renamed it `finish(&mut
//! self)` so the stack buffer is not moved, and this copy uses that
//! signature. Which octets are written where, and every allocation, are
//! 0.5.0's.

/// How many octets a body is written on the stack before any heap is touched.
pub const STACK_OCTETS: usize = 1024;

/// Where Kynos serializes a response body: on the stack until it outgrows
/// it, then on the heap, starting at twice the stack.
///
/// No hint is carried between responses: the first costs what the thousandth
/// does.
pub struct BodyWriter {
    stack: [u8; STACK_OCTETS],
    len: usize,
    spilled: Vec<u8>,
}

impl std::fmt::Debug for BodyWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BodyWriter")
            .field("stack", &self.len)
            .field("spilled", &self.spilled.len())
            .finish()
    }
}

impl Default for BodyWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl BodyWriter {
    /// An empty writer, with nothing allocated.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stack: [0; STACK_OCTETS],
            len: 0,
            spilled: Vec::new(),
        }
    }

    /// What was written, as a body: the stack's octets copied into a `Bytes`
    /// of exactly their length, or the spilled heap buffer itself.
    pub fn finish(&mut self) -> bytes::Bytes {
        if self.spilled.is_empty() {
            bytes::Bytes::copy_from_slice(&self.stack[..self.len])
        } else {
            bytes::Bytes::from(std::mem::take(&mut self.spilled))
        }
    }

    /// Moves what the stack holds to the heap, with room for `more` after it.
    #[cold]
    #[inline(never)]
    fn spill(&mut self, more: usize) {
        self.spilled = Vec::with_capacity((self.len + more).max(2 * STACK_OCTETS));
        self.spilled.extend_from_slice(&self.stack[..self.len]);
    }
}

// Inlined, as `Vec<u8>`'s are: `serde_json` writes in pieces of a few octets,
// and an out-of-line call per piece costs more than the piece.
impl std::io::Write for BodyWriter {
    #[inline]
    fn write(&mut self, octets: &[u8]) -> std::io::Result<usize> {
        self.write_all(octets)?;
        Ok(octets.len())
    }

    #[inline]
    fn write_all(&mut self, octets: &[u8]) -> std::io::Result<()> {
        if self.spilled.is_empty() {
            if let Some(free) = self.stack.get_mut(self.len..self.len + octets.len()) {
                free.copy_from_slice(octets);
                self.len += octets.len();
                return Ok(());
            }
            self.spill(octets.len());
        }
        self.spilled.extend_from_slice(octets);
        Ok(())
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
