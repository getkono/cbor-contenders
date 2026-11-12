//! `byte-strings`: opaque binary blobs, the case for a binary format.
//!
//! A blob is a CBOR byte string (major type 2) in every CBOR encoding, and
//! base64 text in JSON, as a JSON API sends bytes. One field type does both:
//! [`wire`] asks the serializer whether it is human-readable, which serde_json
//! is and the CBOR serializers are not.

use serde::{Deserialize, Serialize};

use crate::families::FAMILY_BYTES;
use crate::{Group, Rng, Workload, cbor};

/// Bytes as a byte string to a binary serializer and standard base64 text to
/// a human-readable one.
pub mod wire {
    use std::borrow::Cow;

    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    const BASE64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

    /// Write `bytes` as the format writes bytes.
    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_str(&BASE64.encode(bytes))
        } else {
            serializer.serialize_bytes(bytes)
        }
    }

    /// Read bytes as the format writes them, into an owned buffer.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        if deserializer.is_human_readable() {
            let text = <Cow<'de, str>>::deserialize(deserializer)?;
            BASE64.decode(text.as_bytes()).map_err(D::Error::custom)
        } else {
            serde_bytes::deserialize(deserializer)
        }
    }

    /// Read bytes as a slice of the input. A human-readable format holds
    /// them as base64 text, which has no slice of the input to hand out, so
    /// it is refused there: that format cannot borrow bytes.
    pub fn borrow<'de: 'a, 'a, D: Deserializer<'de>>(deserializer: D) -> Result<&'a [u8], D::Error> {
        if deserializer.is_human_readable() {
            Err(D::Error::custom("bytes written as base64 text cannot be borrowed"))
        } else {
            <&'a [u8]>::deserialize(deserializer)
        }
    }
}

/// One blob.
#[derive(Clone, PartialEq, Serialize, Deserialize, prost::Message)]
#[cfg_attr(feature = "minicbor", derive(minicbor::Encode, minicbor::Decode))]
pub struct Blob {
    /// What it is.
    #[prost(string, tag = "1")]
    #[cfg_attr(feature = "minicbor", n(0))]
    pub name: String,
    /// The bytes.
    #[serde(with = "wire")]
    #[prost(bytes = "vec", tag = "2")]
    #[cfg_attr(feature = "minicbor", n(1), cbor(with = "minicbor::bytes"))]
    pub data: Vec<u8>,
}

/// The `byte-strings` body.
#[derive(Clone, PartialEq, Serialize, Deserialize, prost::Message)]
#[cfg_attr(feature = "minicbor", derive(minicbor::Encode, minicbor::Decode))]
pub struct Blobs {
    /// The blobs.
    #[prost(message, repeated, tag = "1")]
    #[cfg_attr(feature = "minicbor", n(0))]
    pub blobs: Vec<Blob>,
}

codecs::native_type!(Blobs);

/// [`Blob`], borrowing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlobRef<'a> {
    /// What it is.
    pub name: &'a str,
    /// The bytes: a byte string borrowed from a CBOR body. Refused from a
    /// JSON body, whose base64 text has no bytes to borrow.
    #[serde(borrow, serialize_with = "wire::serialize", deserialize_with = "wire::borrow")]
    pub data: &'a [u8],
}

/// [`Blobs`], borrowing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlobsRef<'a> {
    /// The blobs.
    #[serde(borrow)]
    pub blobs: Vec<BlobRef<'a>>,
}

/// `byte-strings`: about 16 KiB of CBOR, blobs of 16 to 512 random bytes.
#[derive(Debug)]
pub struct ByteStrings;

impl Workload for ByteStrings {
    const NAME: &'static str = "byte-strings";
    const GROUP: Group = Group::Family;
    type Owned = Blobs;
    type Borrowed<'de> = BlobsRef<'de>;
    type Native = Blobs;

    fn native() -> Option<Blobs> {
        Some(Self::value())
    }

    fn value() -> Blobs {
        let mut rng = Rng::new(0xb10b);
        let mut blobs = Blobs { blobs: Vec::new() };
        while cbor(&blobs).len() < FAMILY_BYTES {
            let len = 16 + rng.below(497) as usize;
            let index = blobs.blobs.len();
            blobs.blobs.push(Blob {
                name: format!("blob-{index:04}"),
                data: (0..len).map(|_| rng.below(256) as u8).collect(),
            });
        }
        blobs
    }
}
