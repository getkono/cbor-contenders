//! Section `floats`: half, single and double precision, `-0`, infinities,
//! NaN payloads, subnormals, and a million random values of each width
//! round-tripped bit for bit through the library's own encoder and decoder.
//!
//! Gates: a finite value or an infinity decoded to other bits (the sign of
//! `-0` included), a width refused into a target that holds it exactly, any
//! random value that does not round-trip, and `-0`, `+inf` or `-inf` encoded
//! as `f32` or `f64` and read back as another value by the library or the
//! oracle. Recorded: whether a NaN keeps its payload and sign (RFC 8949
//! section 4.2.2 leaves NaN's bits to the application), how a double that
//! does not fit is narrowed into `f32`, and the bytes and head width (`f9`,
//! `fa`, `fb`) each encoder writes for `-0`, the infinities and NaN.
//!
//! [`run`]'s gating checks, with a thousand random values, are tested on
//! the oracle in this module.

use codecs::Backend;
use payloads::Rng;
use serde_json::{Value, json};

use crate::cases::{FloatBits, decode, hex};
use crate::outcome::{Outcome, guarded};
use crate::record::Section;

/// Random values per width.
pub const SAMPLES: usize = 1_000_000;

/// `(what, bytes, target, bits it must decode to)`.
const EXACT: &[(&str, &str, &str, &str)] = &[
    ("half 1.0 into f64", "f9 3c00", "f64", "0x3ff0000000000000"),
    ("half -0.0 into f64", "f9 8000", "f64", "0x8000000000000000"),
    ("half 65504 into f64", "f9 7bff", "f64", "0x40effc0000000000"),
    (
        "half smallest subnormal into f64",
        "f9 0001",
        "f64",
        "0x3e70000000000000",
    ),
    ("half +inf into f64", "f9 7c00", "f64", "0x7ff0000000000000"),
    ("half -inf into f64", "f9 fc00", "f64", "0xfff0000000000000"),
    ("half 1.5 into f32", "f9 3e00", "f32", "0x3ff8000000000000"),
    ("single 100000.0 into f64", "fa 47c35000", "f64", "0x40f86a0000000000"),
    (
        "single smallest subnormal into f64",
        "fa 00000001",
        "f64",
        "0x36a0000000000000",
    ),
    ("single -0.0 into f32", "fa 80000000", "f32", "0x8000000000000000"),
    (
        "double 1.1 into f64",
        "fb 3ff199999999999a",
        "f64",
        "0x3ff199999999999a",
    ),
    (
        "double smallest subnormal into f64",
        "fb 0000000000000001",
        "f64",
        "0x0000000000000001",
    ),
    (
        "double -0.0 into f64",
        "fb 8000000000000000",
        "f64",
        "0x8000000000000000",
    ),
    (
        "double 1.5 into f32",
        "fb 3ff8000000000000",
        "f32",
        "0x3ff8000000000000",
    ),
];

/// Recorded: NaNs with payloads and signs, and a narrowing.
const RECORDED: &[(&str, &str, &str)] = &[
    ("half quiet NaN", "f9 7e00", "f64"),
    ("half NaN with payload", "f9 7e01", "f64"),
    ("half negative NaN", "f9 fe00", "f64"),
    ("single signalling NaN", "fa 7f800001", "f64"),
    ("double NaN with payload", "fb 7ff8000000000123", "f64"),
    ("double negative NaN", "fb fff8000000000000", "f64"),
    ("double 1.1 into f32 (inexact)", "fb 3ff199999999999a", "f32"),
    ("double 1e300 into f32 (out of range)", "fb 7e37e43c8800759c", "f32"),
    ("an integer into f64", "01", "f64"),
];

/// NaN payloads, read back as bits: `cases::Bits` collapses every NaN.
fn nan_bits<B: Backend>(bytes: &[u8]) -> Value {
    match guarded(|| B::decode::<f64>(bytes::Bytes::copy_from_slice(bytes))) {
        Ok(Ok(value)) => json!(format!("0x{:016x}", value.to_bits())),
        Ok(Err(failure)) => json!({ "reject": failure.status() }),
        Err(panic) => json!({ "panic": panic }),
    }
}

/// Whether `value` survives `B`'s encode and decode bit for bit.
fn round_trips<B: Backend, T: serde::Serialize + serde::de::DeserializeOwned + Copy + FloatBits>(value: T) -> bool {
    guarded(|| {
        B::encode(&value)
            .and_then(|bytes| B::decode::<T>(bytes.into()))
            .is_ok_and(|back: T| back.bits() == value.bits())
    })
    .unwrap_or(false)
}

/// The width a float head names.
fn width(head: Option<&u8>) -> &'static str {
    match head {
        Some(0xf9) => "half (f9)",
        Some(0xfa) => "single (fa)",
        Some(0xfb) => "double (fb)",
        _ => "not a float",
    }
}

/// One special value encoded by `B` and read back by `B` and the oracle as
/// the same type: the report's record, and why it gates, if it does (only
/// for a value that is not NaN: NaN's bits are the application's).
fn special<B: Backend, T>(value: T) -> (Value, Option<String>)
where
    T: serde::Serialize + serde::de::DeserializeOwned + Copy + FloatBits,
{
    let bytes = match guarded(|| B::encode(&value)) {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(failure)) => {
            return (
                json!({ "refused": failure.to_string() }),
                Some(format!("encode failed: {failure}")),
            );
        }
        Err(panic) => return (json!({ "panic": panic }), Some(format!("encode panicked: {panic}"))),
    };
    let own = guarded(|| B::decode::<T>(bytes::Bytes::copy_from_slice(&bytes)))
        .ok()
        .and_then(Result::ok);
    let oracle = serde_cbor::from_slice::<T>(&bytes).ok();
    let exact = |back: Option<T>| back.is_some_and(|back| back.bits() == value.bits());
    let nan = value.bits() == json!("nan");
    let raw = |back: Option<T>| back.map_or_else(|| json!("refused"), |back| back.bits());
    let failure = match (exact(own), exact(oracle)) {
        _ if nan => None,
        (false, _) => Some("the library reads its own encoding back as another value".to_owned()),
        (_, false) => Some("the oracle reads the encoding as another value".to_owned()),
        _ => None,
    };
    (
        json!({
            "hex": hex::encode(&bytes),
            "width": width(bytes.first()),
            "own": raw(own),
            "oracle": raw(oracle),
        }),
        failure,
    )
}

/// `-0`, `±inf` and NaN encoded as `f32` and as `f64`: the bytes and head
/// width recorded, and a `-0` or infinity read back as another value gated.
fn special_encodes<B: Backend>(section: &mut Section) {
    let mut encoded = serde_json::Map::new();
    let cases: [(&str, (Value, Option<String>)); 8] = [
        ("-0.0 as f64", special::<B, f64>(-0.0)),
        ("+inf as f64", special::<B, f64>(f64::INFINITY)),
        ("-inf as f64", special::<B, f64>(f64::NEG_INFINITY)),
        ("NaN as f64", special::<B, f64>(f64::NAN)),
        ("-0.0 as f32", special::<B, f32>(-0.0)),
        ("+inf as f32", special::<B, f32>(f32::INFINITY)),
        ("-inf as f32", special::<B, f32>(f32::NEG_INFINITY)),
        ("NaN as f32", special::<B, f32>(f32::NAN)),
    ];
    for (what, (record, failure)) in cases {
        let case = format!("encode {what}");
        section.case();
        if let Some(failure) = failure {
            section.differ(&case, json!("the same value"), record.clone());
            section.fail(&case, failure);
        }
        encoded.insert(what.to_owned(), record);
    }
    section.data("encoded_specials", Value::Object(encoded));
}

/// Run the section.
#[must_use]
pub fn run<B: Backend>(samples: usize) -> Section {
    let mut section = Section::new("floats");
    for (what, text, target, bits) in EXACT {
        let case = format!("{what} ({text})");
        section.case();
        let outcome = decode::<B>(target, &hex(text));
        if !matches!(&outcome, Outcome::Accept(Value::String(got)) if got == bits) {
            section.differ(&case, json!(bits), outcome.describe());
            section.fail(
                &case,
                if outcome.is_accept() {
                    "decoded to other bits"
                } else {
                    "a float refused"
                },
            );
        }
    }
    let mut observed = serde_json::Map::new();
    for (what, text, target) in RECORDED {
        section.case();
        let bytes = hex(text);
        let outcome = if *target == "f64" && what.contains("NaN") {
            nan_bits::<B>(&bytes)
        } else {
            decode::<B>(target, &bytes).describe()
        };
        observed.insert(format!("{what} ({text})"), outcome);
    }
    section.data("recorded", Value::Object(observed));
    if B::ENCODES {
        special_encodes::<B>(&mut section);
    }
    if B::ENCODES {
        let mut rng = Rng::new(0xf107);
        let mut failed = [0usize; 2];
        for i in 0..samples {
            let double = rng.finite_f64();
            let single = rng.finite_f32();
            section.case();
            if !round_trips::<B, f64>(double) {
                failed[0] += 1;
                section.differ(&format!("f64 #{i}"), json!(double), json!("did not round-trip"));
            }
            section.case();
            if !round_trips::<B, f32>(single) {
                failed[1] += 1;
                section.differ(&format!("f32 #{i}"), json!(single), json!("did not round-trip"));
            }
        }
        for (width, count) in ["f64", "f32"].iter().zip(failed) {
            if count > 0 {
                section.fail(
                    &format!("random {width}"),
                    format!("{count} of {samples} did not round-trip"),
                );
            }
        }
        section.data(
            "round_trip_failures",
            json!({ "f64": failed[0], "f32": failed[1], "samples": samples }),
        );
    }
    section
}

#[cfg(test)]
mod tests {
    use codecs::backend::serde_cbor::SerdeCbor;

    use super::*;

    #[test]
    fn the_oracle_passes_floats_and_writes_specials_short() {
        let section = run::<SerdeCbor>(1_000);
        assert_eq!(section.failure_count(), 0, "{:#?}", section.failures());
        let (record, _) = section.finish();
        let specials = &record["data"]["encoded_specials"];
        assert_eq!(specials["-0.0 as f64"]["hex"], "f98000", "{specials}");
        assert_eq!(specials["+inf as f32"]["width"], "half (f9)", "{specials}");
    }
}
