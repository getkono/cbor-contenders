//! prost 0.14.4, the `Protobuf<T>` baseline: what Kynos's own protobuf body
//! calls (`Message::decode` on the received `Bytes`, `encode_to_vec`).
//!
//! `encode_to_vec` sizes its buffer by `encoded_len` and writes it once, so
//! it never reallocates; `encode_into` reserves the same length in the
//! caller's buffer before writing.

use bytes::Bytes;

use crate::{Failure, Format, Native, NativeType};

/// `prost::Message::{decode, encode_to_vec}`, and `encode_raw` into a
/// buffer reserved to `encoded_len`.
#[derive(Debug)]
pub struct Prost;

impl Native for Prost {
    const NAME: &'static str = "prost";
    const CRATE: &'static str = "prost";
    const FORMAT: Format = Format::Protobuf;
    const ELIGIBLE: bool = true;

    fn decode<T: NativeType>(body: Bytes) -> Result<T, Failure> {
        T::proto_decode(body)
    }

    fn encode<T: NativeType>(value: &T) -> Result<Vec<u8>, Failure> {
        value.proto_to_vec()
    }

    fn encode_into<T: NativeType>(value: &T, out: &mut Vec<u8>) -> Result<(), Failure> {
        out.clear();
        value.proto_encode(out)
    }

    fn encode_server<T: NativeType>(value: &T) -> Result<Vec<u8>, Failure> {
        value.proto_to_vec()
    }
}
