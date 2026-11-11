//! Shared helpers for hand-built cases.

use codecs::Backend;
use codecs::body::Arrival;
use serde::de::IgnoredAny;
use serde_json::Value;

use crate::outcome::{AsValue, Decode, Ignored, Outcome, Typed, owned};

/// Bytes from hex, whitespace ignored.
///
/// # Panics
/// On text that is not hex, which is a bug in a case table.
#[must_use]
pub fn hex(text: &str) -> Vec<u8> {
    let text: String = text.split_whitespace().collect();
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap_or_else(|_| panic!("not hex: {text}")))
        .collect()
}

/// The decode paths by target name: what a case is decoded into.
#[must_use]
pub fn path<B: Backend>(target: &str) -> Option<Decode> {
    Some(match target {
        "u8" => owned::<B, Typed<u8>>,
        "u64" => owned::<B, Typed<u64>>,
        "i64" => owned::<B, Typed<i64>>,
        "u128" => owned::<B, Wide<u128>>,
        "i128" => owned::<B, Wide<i128>>,
        "f32" => owned::<B, Bits<f32>>,
        "f64" => owned::<B, Bits<f64>>,
        "bool" => owned::<B, Typed<bool>>,
        "option" => owned::<B, Typed<Option<u8>>>,
        "string" => owned::<B, Typed<String>>,
        "bytes" => owned::<B, HexBytes>,
        "byte-vec" => owned::<B, Typed<Vec<u8>>>,
        "value" => owned::<B, AsValue>,
        "int-map" => owned::<B, Typed<std::collections::BTreeMap<u64, u64>>>,
        "text-map" => owned::<B, Typed<std::collections::BTreeMap<String, u32>>>,
        "ignored" => owned::<B, Ignored>,
        _ => return None,
    })
}

/// Decode `bytes` (as a unique body) into `target` through `B`.
#[must_use]
pub fn decode<B: Backend>(target: &str, bytes: &[u8]) -> Outcome {
    path::<B>(target).map_or_else(
        || Outcome::Panic(format!("no target {target}")),
        |decode| decode(bytes, Arrival::Unique),
    )
}

/// A 128-bit integer, rendered as its decimal digits (no JSON number holds
/// one exactly).
#[derive(Debug)]
pub struct Wide<T>(std::marker::PhantomData<T>);

impl<T: serde::de::DeserializeOwned + std::fmt::Display> crate::outcome::Target for Wide<T> {
    type Out<'de> = T;
    fn view(out: &T) -> Value {
        Value::String(out.to_string())
    }
}

/// A float rendered as its bit pattern, `"nan"` for any NaN.
#[derive(Debug)]
pub struct Bits<T>(std::marker::PhantomData<T>);

/// A float's bit pattern as `0x...`, `"nan"` for any NaN, widened from `f32`.
pub trait FloatBits: serde::de::DeserializeOwned {
    /// The rendering.
    fn bits(&self) -> Value;
}

impl FloatBits for f64 {
    fn bits(&self) -> Value {
        if self.is_nan() {
            Value::String("nan".into())
        } else {
            Value::String(format!("0x{:016x}", self.to_bits()))
        }
    }
}

impl FloatBits for f32 {
    fn bits(&self) -> Value {
        f64::from(*self).bits()
    }
}

impl<T: FloatBits> crate::outcome::Target for Bits<T> {
    type Out<'de> = T;
    fn view(out: &T) -> Value {
        out.bits()
    }
}

/// Bytes through `serde_bytes`, rendered as hex.
#[derive(Debug)]
pub struct HexBytes;

impl crate::outcome::Target for HexBytes {
    type Out<'de> = serde_bytes::ByteBuf;
    fn view(out: &serde_bytes::ByteBuf) -> Value {
        Value::String(hex::encode(out.as_slice()))
    }
}

/// `IgnoredAny`, re-exported for case tables.
pub type Skip = IgnoredAny;
