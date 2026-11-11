//! Section `floats`: half, single and double precision, `-0`, infinities,
//! NaN payloads, subnormals, and a million random values of each width
//! round-tripped bit for bit through the library's own encoder and decoder.
//!
//! Gates: a finite value or an infinity decoded to other bits (the sign of
//! `-0` included), a width refused into a target that holds it exactly, and
//! any random value that does not round-trip. Recorded: whether a NaN keeps
//! its payload and sign (RFC 8949 section 4.2.2 leaves NaN's bits to the
//! application), and how a double that does not fit is narrowed into `f32`.

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
