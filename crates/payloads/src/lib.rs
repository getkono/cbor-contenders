//! Every workload the comparison runs, as typed structs.
//!
//! A workload is a Rust type plus the byte string that encodes it in each
//! format. The bytes are a pure function of this crate: generators draw from
//! [`Rng`], a fixed splitmix64 stream, never from a clock or an RNG crate, so
//! a dependency bump cannot move an input. `tests` pins every corpus by
//! SHA-256, per format.
//!
//! The CBOR bytes are serde_cbor's `to_vec`: preferred serialization (RFC
//! 8949 section 4.1), from a library none of the candidates shares code with.
//! The JSON bytes are serde_json's compact encoding, the protobuf bytes
//! prost's, and minicbor-native's minicbor's own derive.
//!
//! Each workload has an owned form (`String` fields) and a borrowed form
//! (`#[serde(borrow)] Cow<'de, str>` wherever the owned form holds a string).
//! The borrowed form borrows wherever the backend can hand out a slice of the
//! input and allocates otherwise, so the difference between the two forms *is*
//! each backend's ability to borrow. Workloads with no strings use one type
//! for both forms.
//!
//! The native formats (prost, minicbor's derive) need a second derive, so
//! only the workloads whose shape they can carry have a [`Workload::native`]
//! value: a type deriving both, often a one-field message holding the list a
//! serde workload has at its root, since a protobuf message cannot be a bare
//! list.
//!
//! Most workloads' bytes decode to their value. The `reject-*` workloads'
//! bytes must be refused ([`Expect::Reject`]): they measure what a server pays
//! to turn a bad body away.

pub mod blob;
pub mod enums;
pub mod families;
pub mod kynos;
pub mod reject;
pub mod rng;
pub mod sweep;
pub mod world;

use codecs::{Format, NativeType};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub use rng::Rng;

/// A workload: a typed value and the bytes that encode it.
pub trait Workload: 'static {
    /// Stable identifier used in result files and on the command line.
    const NAME: &'static str;
    /// Which group the workload reports under.
    const GROUP: Group;
    /// The owned form.
    type Owned: Serialize + DeserializeOwned + std::fmt::Debug;
    /// The borrowed form.
    type Borrowed<'de>: Serialize + Deserialize<'de> + std::fmt::Debug;
    /// The type the native formats decode into: [`NotCarried`] where they
    /// cannot carry the workload.
    type Native: NativeType + PartialEq + std::fmt::Debug;

    /// What decoding the bytes must yield.
    const EXPECT: Expect = Expect::Value;

    /// The value the bytes encode. For a workload whose bytes must be
    /// rejected, the value they were damaged from.
    #[must_use]
    fn value() -> Self::Owned;

    /// The same value as the native formats hold it, if they can.
    #[must_use]
    fn native() -> Option<Self::Native> {
        None
    }

    /// The input bytes in `format`; empty for a native format that does not
    /// carry the workload.
    #[must_use]
    fn bytes(format: Format) -> Vec<u8> {
        match format {
            Format::Json | Format::Cbor => encode(format, &Self::value()),
            Format::Protobuf | Format::MinicborNative => {
                Self::native().map_or_else(Vec::new, |value| encode_native(format, &value))
            }
        }
    }
}

/// What a correct decode of a workload's bytes yields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// [`Workload::value`].
    Value,
    /// A refusal a server answers with this status: 400 for a body that is
    /// not a valid item or ends early, 422 for a valid item of the wrong
    /// shape.
    Reject(u16),
}

/// Report grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Group {
    /// The three shapes the decision rule is evaluated on.
    Kynos,
    /// The size sweep.
    Sweep,
    /// One variable at a time.
    Family,
    /// Schema-equivalents of widely used real-world documents.
    World,
}

/// A native value for a workload the native formats cannot carry: it has no
/// value, so no bytes, and nothing is measured for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotCarried;

impl NativeType for NotCarried {
    fn proto_decode(_: bytes::Bytes) -> Result<Self, codecs::Failure> {
        Err(codecs::Failure::unsupported())
    }
    fn proto_encode(&self, _: &mut Vec<u8>) -> Result<(), codecs::Failure> {
        Err(codecs::Failure::unsupported())
    }
    fn mini_decode(_: &[u8]) -> Result<Self, codecs::Failure> {
        Err(codecs::Failure::unsupported())
    }
    fn mini_encode(&self, _: &mut Vec<u8>) -> Result<(), codecs::Failure> {
        Err(codecs::Failure::unsupported())
    }
}

/// serde_json's compact encoding.
///
/// # Panics
/// If a generated value does not serialize, which is a bug in this crate.
#[must_use]
pub fn json<T: Serialize>(value: &T) -> Vec<u8> {
    match serde_json::to_vec(value) {
        Ok(bytes) => bytes,
        Err(error) => unreachable!("a generated value failed to serialize: {error}"),
    }
}

/// serde_cbor's encoding: preferred serialization.
///
/// # Panics
/// As [`json`].
#[must_use]
pub fn cbor<T: Serialize>(value: &T) -> Vec<u8> {
    match serde_cbor::to_vec(value) {
        Ok(bytes) => bytes,
        Err(error) => unreachable!("a generated value failed to serialize: {error}"),
    }
}

/// `value` in a serde format.
///
/// # Panics
/// For a native format, which has its own types ([`encode_native`]).
#[must_use]
pub fn encode<T: Serialize>(format: Format, value: &T) -> Vec<u8> {
    match format {
        Format::Json => json(value),
        Format::Cbor => cbor(value),
        Format::Protobuf | Format::MinicborNative => unreachable!("{format:?} is not a serde format"),
    }
}

/// `value` in a native format; empty in a build without that format.
///
/// # Panics
/// For a serde format, or if a generated value does not encode.
#[must_use]
pub fn encode_native<T: NativeType>(format: Format, value: &T) -> Vec<u8> {
    let mut out = Vec::new();
    let encoded = match format {
        Format::Protobuf => value.proto_encode(&mut out),
        Format::MinicborNative => value.mini_encode(&mut out),
        Format::Json | Format::Cbor => unreachable!("{format:?} is not a native format"),
    };
    match encoded {
        Ok(()) => out,
        Err(failure) if failure.class == codecs::Class::Unsupported => Vec::new(),
        Err(failure) => unreachable!("a generated value failed to encode: {failure}"),
    }
}

/// Append a CBOR head of `major` type and argument `n`, in its shortest form.
pub fn head(out: &mut Vec<u8>, major: u8, n: u64) {
    let major = major << 5;
    match n {
        0..=23 => out.push(major | n as u8),
        24..=0xff => out.extend_from_slice(&[major | 0x18, n as u8]),
        0x100..=0xffff => {
            out.push(major | 0x19);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        0x1_0000..=0xffff_ffff => {
            out.push(major | 0x1a);
            out.extend_from_slice(&(n as u32).to_be_bytes());
        }
        _ => {
            out.push(major | 0x1b);
            out.extend_from_slice(&n.to_be_bytes());
        }
    }
}

/// Receives a workload type chosen at run time.
pub trait Visit {
    /// What the visit produces.
    type Output;
    /// Called with the workload named on the command line.
    fn visit<W: Workload>(self) -> Self::Output;
}

/// Every workload, in report order.
pub const ALL: &[&str] = &[
    kynos::JsonSmall::NAME,
    kynos::EchoPost::NAME,
    kynos::JsonLarge::NAME,
    sweep::Sweep::<64>::NAME,
    sweep::Sweep::<256>::NAME,
    sweep::Sweep::<1024>::NAME,
    sweep::Sweep::<4096>::NAME,
    sweep::Sweep::<16384>::NAME,
    sweep::Sweep::<65536>::NAME,
    sweep::Sweep::<262_144>::NAME,
    sweep::Sweep::<1_048_576>::NAME,
    families::Ints::NAME,
    families::Floats::NAME,
    families::Unicode::NAME,
    families::Nested::<100>::NAME,
    families::Nested::<255>::NAME,
    families::Nested::<257>::NAME,
    families::Nested::<10_000>::NAME,
    families::Wide::NAME,
    families::Points::NAME,
    families::IntsExtremes::NAME,
    families::F32s::NAME,
    families::LongString::NAME,
    families::FloatsShort::NAME,
    families::Sparse::NAME,
    families::Defaults::NAME,
    families::KeysLongInt::NAME,
    blob::ByteStrings::NAME,
    families::NumericArrays::NAME,
    enums::Enums::NAME,
    world::TwitterLike::NAME,
    world::CitmLike::NAME,
    world::CanadaLike::NAME,
    world::SkipUnknown::NAME,
    reject::RejectTruncated::NAME,
    reject::RejectShape::NAME,
];

/// Dispatch `visitor` to the workload called `name`.
pub fn dispatch<V: Visit>(name: &str, visitor: V) -> Option<V::Output> {
    Some(match name {
        kynos::JsonSmall::NAME => visitor.visit::<kynos::JsonSmall>(),
        kynos::EchoPost::NAME => visitor.visit::<kynos::EchoPost>(),
        kynos::JsonLarge::NAME => visitor.visit::<kynos::JsonLarge>(),
        sweep::Sweep::<64>::NAME => visitor.visit::<sweep::Sweep<64>>(),
        sweep::Sweep::<256>::NAME => visitor.visit::<sweep::Sweep<256>>(),
        sweep::Sweep::<1024>::NAME => visitor.visit::<sweep::Sweep<1024>>(),
        sweep::Sweep::<4096>::NAME => visitor.visit::<sweep::Sweep<4096>>(),
        sweep::Sweep::<16384>::NAME => visitor.visit::<sweep::Sweep<16384>>(),
        sweep::Sweep::<65536>::NAME => visitor.visit::<sweep::Sweep<65536>>(),
        sweep::Sweep::<262_144>::NAME => visitor.visit::<sweep::Sweep<262_144>>(),
        sweep::Sweep::<1_048_576>::NAME => visitor.visit::<sweep::Sweep<1_048_576>>(),
        families::Ints::NAME => visitor.visit::<families::Ints>(),
        families::Floats::NAME => visitor.visit::<families::Floats>(),
        families::Unicode::NAME => visitor.visit::<families::Unicode>(),
        families::Nested::<100>::NAME => visitor.visit::<families::Nested<100>>(),
        families::Nested::<255>::NAME => visitor.visit::<families::Nested<255>>(),
        families::Nested::<257>::NAME => visitor.visit::<families::Nested<257>>(),
        families::Nested::<10_000>::NAME => visitor.visit::<families::Nested<10_000>>(),
        families::Wide::NAME => visitor.visit::<families::Wide>(),
        families::Points::NAME => visitor.visit::<families::Points>(),
        families::IntsExtremes::NAME => visitor.visit::<families::IntsExtremes>(),
        families::F32s::NAME => visitor.visit::<families::F32s>(),
        families::LongString::NAME => visitor.visit::<families::LongString>(),
        families::FloatsShort::NAME => visitor.visit::<families::FloatsShort>(),
        families::Sparse::NAME => visitor.visit::<families::Sparse>(),
        families::Defaults::NAME => visitor.visit::<families::Defaults>(),
        families::KeysLongInt::NAME => visitor.visit::<families::KeysLongInt>(),
        blob::ByteStrings::NAME => visitor.visit::<blob::ByteStrings>(),
        families::NumericArrays::NAME => visitor.visit::<families::NumericArrays>(),
        enums::Enums::NAME => visitor.visit::<enums::Enums>(),
        world::TwitterLike::NAME => visitor.visit::<world::TwitterLike>(),
        world::CitmLike::NAME => visitor.visit::<world::CitmLike>(),
        world::CanadaLike::NAME => visitor.visit::<world::CanadaLike>(),
        world::SkipUnknown::NAME => visitor.visit::<world::SkipUnknown>(),
        reject::RejectTruncated::NAME => visitor.visit::<reject::RejectTruncated>(),
        reject::RejectShape::NAME => visitor.visit::<reject::RejectShape>(),
        _ => return None,
    })
}

/// Whether the native formats carry the workload called `name`: the matrix
/// measures prost and minicbor's derive only where they do.
#[must_use]
pub fn carries_native(name: &str) -> bool {
    struct Carried;
    impl Visit for Carried {
        type Output = bool;
        fn visit<W: Workload>(self) -> bool {
            W::native().is_some()
        }
    }
    dispatch(name, Carried).unwrap_or(false)
}

#[cfg(test)]
mod tests;
