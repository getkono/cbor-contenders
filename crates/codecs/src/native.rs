//! Formats with their own derive: prost (the `Protobuf<T>` baseline) and
//! minicbor's `Encode`/`Decode`.
//!
//! These decode and encode only types that implement [`NativeType`], which a
//! workload's type does through [`native_type!`](crate::native_type) when the
//! format can carry it. A workload whose types it cannot carry (an enum with
//! data for prost, a borrowed form for either) is not measured for it, and
//! the matrix says so (`payloads::NATIVE`).
//!
//! # The server's own encode
//!
//! [`Native::encode_server`] is the encode a server built on the format
//! makes, and the `encode-writer` shape measures it for a native entry. For
//! prost it is `Message::encode_to_vec`, which Kynos's `Protobuf<T>` response
//! calls (`kynos-0.5.0/src/response/codec/protobuf.rs`); for minicbor's
//! derive, `minicbor::to_vec`, the library's own entry point. A serde entry's
//! counterpart is [`Backend::to_writer`](crate::Backend::to_writer) into a
//! [`BodyWriter`](crate::writer::BodyWriter) and then
//! [`finish`](crate::writer::BodyWriter::finish), which is what Kynos does
//! for `Json<T>` and `Encoded<C, T>`.

pub mod prost;

#[cfg(feature = "minicbor")]
pub mod minicbor;

use bytes::Bytes;

use crate::{Failure, Format};

/// A type a native format can carry.
pub trait NativeType: Sized {
    /// Decode as a protobuf message.
    fn proto_decode(body: Bytes) -> Result<Self, Failure>;
    /// Append the protobuf encoding to `out`.
    fn proto_encode(&self, out: &mut Vec<u8>) -> Result<(), Failure>;
    /// Decode with minicbor's derive.
    fn mini_decode(body: &[u8]) -> Result<Self, Failure>;
    /// Append minicbor's derived encoding to `out`.
    fn mini_encode(&self, out: &mut Vec<u8>) -> Result<(), Failure>;

    /// The protobuf encoding in a new buffer. [`native_type!`](crate::native_type)
    /// makes it prost's `encode_to_vec`; the default appends to an empty
    /// `Vec`, for a type that carries nothing.
    fn proto_to_vec(&self) -> Result<Vec<u8>, Failure> {
        let mut out = Vec::new();
        self.proto_encode(&mut out)?;
        Ok(out)
    }

    /// minicbor's derived encoding in a new buffer. [`native_type!`](crate::native_type)
    /// makes it `minicbor::to_vec`; the default appends to an empty `Vec`.
    fn mini_to_vec(&self) -> Result<Vec<u8>, Failure> {
        let mut out = Vec::new();
        self.mini_encode(&mut out)?;
        Ok(out)
    }
}

/// A native backend, as a server uses one.
pub trait Native: 'static {
    /// Stable identifier used in result files.
    const NAME: &'static str;
    /// The crate providing it.
    const CRATE: &'static str;
    /// The format it reads and writes.
    const FORMAT: Format;
    /// Whether it is a candidate for adoption rather than a reference point.
    const ELIGIBLE: bool;

    /// Decode an owned `T` from a body the server now owns.
    fn decode<T: NativeType>(body: Bytes) -> Result<T, Failure>;
    /// Encode into a new buffer.
    fn encode<T: NativeType>(value: &T) -> Result<Vec<u8>, Failure>;
    /// Encode into `out`, which is cleared first and may already have capacity.
    fn encode_into<T: NativeType>(value: &T, out: &mut Vec<u8>) -> Result<(), Failure>;
    /// Encode as a server built on the format does (module docs): prost's
    /// `encode_to_vec`, as Kynos's `Protobuf<T>` calls it, and
    /// `minicbor::to_vec`.
    fn encode_server<T: NativeType>(value: &T) -> Result<Vec<u8>, Failure>;
}

#[doc(hidden)]
pub mod support {
    //! What [`native_type!`](crate::native_type) expands to.

    use bytes::Bytes;

    use crate::{Class, Failure};

    /// prost's decode. kynos's `Protobuf<T>` answers every prost error 400,
    /// and so does this.
    pub fn proto_decode<T: ::prost::Message + Default>(body: Bytes) -> Result<T, Failure> {
        T::decode(body).map_err(|e| Failure::new(Class::Syntax, e))
    }

    /// prost's encode, appended to `out` with room for it reserved first:
    /// what `encode_to_vec` does (`Vec::with_capacity(encoded_len)`, then
    /// `encode_raw`), into a buffer that may already hold capacity. Calling
    /// `encode` instead would compute `encoded_len` a second time for its
    /// capacity check, which a `Vec` always passes.
    pub fn proto_encode<T: ::prost::Message>(value: &T, out: &mut Vec<u8>) -> Result<(), Failure> {
        out.reserve(value.encoded_len());
        value.encode_raw(out);
        Ok(())
    }

    /// prost's `encode_to_vec`: a buffer of exactly `encoded_len`, written
    /// once. Infallible, as Kynos's `Protobuf<T>` relies on.
    #[must_use]
    pub fn proto_to_vec<T: ::prost::Message>(value: &T) -> Vec<u8> {
        value.encode_to_vec()
    }

    /// minicbor's derived decode.
    #[cfg(feature = "minicbor")]
    pub fn mini_decode<T: for<'b> ::minicbor::Decode<'b, ()>>(body: &[u8]) -> Result<T, Failure> {
        ::minicbor::decode(body).map_err(|e| {
            let class = if e.is_end_of_input() {
                Class::Eof
            } else if e.is_type_mismatch() || e.is_message() || e.is_missing_value() || e.is_unknown_variant() {
                Class::Data
            } else {
                Class::Syntax
            };
            let at = e.position();
            Failure::new(class, e).at(at)
        })
    }

    /// minicbor's derived encode.
    #[cfg(feature = "minicbor")]
    pub fn mini_encode<T: ::minicbor::Encode<()>>(value: &T, out: &mut Vec<u8>) -> Result<(), Failure> {
        ::minicbor::encode(value, out).map_err(|e| Failure::new(Class::Io, e))
    }

    /// `minicbor::to_vec`: an `Encoder` over a new `Vec`.
    #[cfg(feature = "minicbor")]
    pub fn mini_to_vec<T: ::minicbor::Encode<()>>(value: &T) -> Result<Vec<u8>, Failure> {
        ::minicbor::to_vec(value).map_err(|e| Failure::new(Class::Io, e))
    }
}

/// Implement [`NativeType`] for types deriving `prost::Message` and, where
/// the calling crate's `minicbor` feature is on, `minicbor::{Encode, Decode}`.
///
/// The `cfg` is in the expansion, so it is the calling crate's feature that
/// decides (and that feature must enable `codecs/minicbor`): a crate built
/// without minicbor's derive answers `Unsupported` for it whatever else in
/// the build turned minicbor on.
#[macro_export]
macro_rules! native_type {
    ($($ty:ty),* $(,)?) => {$(
        impl $crate::NativeType for $ty {
            fn proto_decode(body: ::bytes::Bytes) -> ::core::result::Result<Self, $crate::Failure> {
                $crate::native::support::proto_decode(body)
            }
            fn proto_encode(&self, out: &mut ::std::vec::Vec<u8>) -> ::core::result::Result<(), $crate::Failure> {
                $crate::native::support::proto_encode(self, out)
            }
            fn proto_to_vec(&self) -> ::core::result::Result<::std::vec::Vec<u8>, $crate::Failure> {
                ::core::result::Result::Ok($crate::native::support::proto_to_vec(self))
            }
            fn mini_decode(body: &[u8]) -> ::core::result::Result<Self, $crate::Failure> {
                #[cfg(feature = "minicbor")]
                return $crate::native::support::mini_decode(body);
                #[cfg(not(feature = "minicbor"))]
                {
                    let _ = body;
                    Err($crate::Failure::unsupported())
                }
            }
            fn mini_encode(&self, out: &mut ::std::vec::Vec<u8>) -> ::core::result::Result<(), $crate::Failure> {
                #[cfg(feature = "minicbor")]
                return $crate::native::support::mini_encode(self, out);
                #[cfg(not(feature = "minicbor"))]
                {
                    let _ = out;
                    Err($crate::Failure::unsupported())
                }
            }
            fn mini_to_vec(&self) -> ::core::result::Result<::std::vec::Vec<u8>, $crate::Failure> {
                #[cfg(feature = "minicbor")]
                return $crate::native::support::mini_to_vec(self);
                #[cfg(not(feature = "minicbor"))]
                Err($crate::Failure::unsupported())
            }
        }
    )*};
}
