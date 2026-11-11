//! prost 0.14.4, the `Protobuf<T>` baseline: what Kynos's own protobuf body
//! calls (`Message::decode` on the received `Bytes`, `encode_to_vec`).

use bytes::Bytes;

use crate::{Failure, Format, Native, NativeType};

/// `prost::Message::{decode, encode_to_vec, encode}`.
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
        let mut out = Vec::new();
        value.proto_encode(&mut out)?;
        Ok(out)
    }

    fn encode_into<T: NativeType>(value: &T, out: &mut Vec<u8>) -> Result<(), Failure> {
        out.clear();
        value.proto_encode(out)
    }
}
