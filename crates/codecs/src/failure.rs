//! Why a decode or encode failed, in the terms a server answers with.

/// What kind of failure, as a server maps it to a status.
///
/// The line between the two statuses is RFC 8949's own: bytes that are not a
/// valid CBOR data item (ill-formed, section 5.1, or invalid, section 5.3:
/// a text string that is not UTF-8, a tag on content it does not admit) are
/// the client's syntax, a 400; a valid item of the wrong shape for the target
/// type is a 422. For JSON, serde_json's own categories draw the same line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Class {
    /// Not a valid data item of the format.
    Syntax,
    /// Valid so far, then the input ended.
    Eof,
    /// A valid item of the wrong shape for the target type.
    Data,
    /// An I/O failure.
    Io,
    /// The backend has no such operation.
    Unsupported,
}

/// A failure, classified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The category.
    pub class: Class,
    /// Zero-based byte offset, if the backend says.
    pub offset: Option<usize>,
    /// The backend's message.
    pub message: String,
}

impl Failure {
    /// A failure of `class` with no position.
    #[must_use]
    pub fn new(class: Class, message: impl std::fmt::Display) -> Self {
        Self {
            class,
            offset: None,
            message: message.to_string(),
        }
    }

    /// Attach a byte offset.
    #[must_use]
    pub fn at(mut self, offset: Option<usize>) -> Self {
        self.offset = offset;
        self
    }

    /// The operation does not exist in this backend.
    #[must_use]
    pub fn unsupported() -> Self {
        Self::new(Class::Unsupported, "unsupported by this backend")
    }

    /// The status a server answers with: 422 for a valid body of the wrong
    /// shape, 400 for everything else.
    #[must_use]
    pub fn status(&self) -> u16 {
        if self.class == Class::Data { 422 } else { 400 }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.class, self.message)
    }
}
