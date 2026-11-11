//! A request body as a server holds it.

use bytes::{Bytes, BytesMut};

/// A body a borrowed decode may borrow from: the `Bytes` the server
/// received, which every backend reads in place.
#[derive(Debug)]
pub struct Body {
    bytes: Bytes,
}

impl Body {
    /// Wrap a received body.
    #[must_use]
    pub fn new(bytes: Bytes) -> Self {
        Self { bytes }
    }

    /// The bytes, read-only.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
}

/// The two ways a server receives a body.
///
/// `Shared` is one HTTP frame split off the connection's read buffer, so it
/// shares that allocation and is not unique. `Unique` is several frames
/// already collected into a fresh buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    /// A single frame sharing the connection buffer.
    Shared,
    /// A unique buffer.
    Unique,
}

impl Arrival {
    /// Stable identifier.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Unique => "unique",
        }
    }

    /// Materialize `input` the way it would arrive. The returned guard keeps
    /// the rest of the shared buffer alive, as the connection would.
    #[must_use]
    pub fn materialize(self, input: &[u8]) -> (Bytes, Option<Bytes>) {
        match self {
            Self::Unique => (Bytes::from(input.to_vec()), None),
            Self::Shared => {
                let mut buffer = BytesMut::with_capacity(input.len() + 64);
                buffer.extend_from_slice(input);
                buffer.extend_from_slice(&[b' '; 64]);
                let mut frozen = buffer.freeze();
                let body = frozen.split_to(input.len());
                (body, Some(frozen))
            }
        }
    }
}
