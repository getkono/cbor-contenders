//! Is a result the value it should be?

use codecs::{Class, Failure, Format, NativeType};
use payloads::{Expect, Workload};
use serde::Serialize;
use serde_json::Value;

/// A result, judged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Identical, floats bit for bit.
    Exact,
    /// Identical except floats within `max_ulps` of the truth (JSON only:
    /// serde_json's default parser does not round every float correctly;
    /// a binary format holds the bits).
    Inexact { floats: u64, max_ulps: u64 },
    /// A different value.
    Wrong(String),
    /// The backend refused the input.
    Rejected(String),
}

impl Verdict {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Inexact { .. } => "inexact",
            Self::Wrong(_) => "wrong",
            Self::Rejected(_) => "rejected",
        }
    }

    /// Whether the result is fit to be measured.
    pub fn measurable(&self) -> bool {
        matches!(self, Self::Exact | Self::Inexact { .. })
    }
}

/// Floats further apart than this are a different value, not a rounding.
const ROUNDING_ULPS: u64 = 4;

fn ulps(a: f64, b: f64) -> u64 {
    let key = |x: f64| {
        let bits = x.to_bits() as i64;
        // Negative floats map below zero in bit order, and -0.0 sits one
        // step below +0.0: they are different values.
        if bits < 0 { -(bits & i64::MAX) - 1 } else { bits }
    };
    key(a).abs_diff(key(b))
}

/// Floats seen inexact so far, and the furthest apart.
struct Tally {
    tolerance: u64,
    floats: u64,
    max_ulps: u64,
}

impl Tally {
    /// Two floats: equal by bits, or a rounding apart (counted), or a
    /// different value.
    fn floats(&mut self, got: f64, expected: f64) -> Result<(), String> {
        if got.to_bits() == expected.to_bits() {
            return Ok(());
        }
        let distance = ulps(got, expected);
        if distance > self.tolerance {
            return Err(format!("{got:e} != {expected:e}"));
        }
        self.floats += 1;
        self.max_ulps = self.max_ulps.max(distance);
        Ok(())
    }

    fn verdict(self, result: Result<(), String>) -> Verdict {
        match result {
            Err(path) => Verdict::Wrong(path),
            Ok(()) if self.floats == 0 => Verdict::Exact,
            Ok(()) => Verdict::Inexact {
                floats: self.floats,
                max_ulps: self.max_ulps,
            },
        }
    }
}

/// Judge a decode of workload `W`'s bytes in `format` against what `W`
/// expects.
///
/// For [`Expect::Value`], a value is compared with [`Workload::value`] and a
/// refusal is `Rejected`. For [`Expect::Reject`], the refusal *is* the
/// result: refused with the expected status is exact; accepted, or refused
/// with another status, is wrong, with the status given in the detail, so a
/// backend that would answer a client wrongly is never measured. A backend
/// without the operation (`Unsupported`) is `Rejected` either way.
pub fn judge_decode<W: Workload, T: Serialize>(format: Format, result: Result<T, Failure>) -> Verdict {
    judge(W::EXPECT, result, |value| {
        compare(format, &to_value(&value), &to_value(&W::value()))
    })
}

/// [`judge_decode`] for a native format, against [`Workload::native`].
pub fn judge_native<W: Workload>(result: Result<W::Native, Failure>) -> Verdict {
    judge(W::EXPECT, result, |value| match W::native() {
        Some(expected) if value == expected => Verdict::Exact,
        Some(_) => Verdict::Wrong("a different value".into()),
        None => Verdict::Rejected("not carried by the native formats".into()),
    })
}

fn judge<T>(expect: Expect, result: Result<T, Failure>, value: impl FnOnce(T) -> Verdict) -> Verdict {
    match (expect, result) {
        (_, Err(failure)) if failure.class == Class::Unsupported => Verdict::Rejected(failure.to_string()),
        (Expect::Value, Ok(decoded)) => value(decoded),
        (Expect::Value, Err(failure)) => Verdict::Rejected(failure.to_string()),
        (Expect::Reject(status), Ok(_)) => Verdict::Wrong(format!("accepted; expected a refusal with status {status}")),
        (Expect::Reject(status), Err(failure)) if failure.status() == status => Verdict::Exact,
        (Expect::Reject(status), Err(failure)) => {
            Verdict::Wrong(format!("answered {}, expected {status}: {failure}", failure.status()))
        }
    }
}

fn to_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or_else(|e| unreachable!("a decoded value re-serializes: {e}"))
}

/// Compare `got` with `expected`, floats to the tolerance `format` allows.
pub fn compare(format: Format, got: &Value, expected: &Value) -> Verdict {
    let mut tally = Tally {
        tolerance: if format == Format::Json { ROUNDING_ULPS } else { 0 },
        floats: 0,
        max_ulps: 0,
    };
    let result = walk(got, expected, &mut tally, &mut String::from("$"));
    tally.verdict(result)
}

fn walk(got: &Value, expected: &Value, tally: &mut Tally, path: &mut String) -> Result<(), String> {
    match (got, expected) {
        (Value::Number(g), Value::Number(e)) if g.is_f64() || e.is_f64() => tally
            .floats(g.as_f64().unwrap_or(f64::NAN), e.as_f64().unwrap_or(f64::NAN))
            .map_err(|difference| format!("{path}: {difference}")),
        (Value::Array(g), Value::Array(e)) if g.len() == e.len() => {
            for (i, (g, e)) in g.iter().zip(e).enumerate() {
                let len = path.len();
                path.push_str(&format!("[{i}]"));
                walk(g, e, tally, path)?;
                path.truncate(len);
            }
            Ok(())
        }
        (Value::Object(g), Value::Object(e)) if g.len() == e.len() => {
            for (key, e) in e {
                let len = path.len();
                path.push_str(&format!(".{key}"));
                walk(g.get(key).ok_or_else(|| format!("{path}: missing"))?, e, tally, path)?;
                path.truncate(len);
            }
            Ok(())
        }
        (g, e) if g == e => Ok(()),
        _ => Err(format!("{path}: {got} != {expected}")),
    }
}

/// Judge an encoder's output for workload `W` in `format` against the
/// reference encoding of the same value, in two steps:
///
/// 1. the same bytes are exact;
/// 2. otherwise the output is read back into `W::Owned` by the format's
///    reference decoder (serde_json for JSON, the serde_cbor oracle for CBOR,
///    the format's own derive for a native format) and compared, so each
///    number is read at its field's type: a float written wider than the
///    shortest exact width reads back as the same float and is exact.
///
/// The oracle stops at 128 levels; an output deeper than that which is not
/// the reference bytes cannot be read back, and is judged wrong with that
/// reason (every encoder measured writes the nested workloads' reference
/// bytes, so none is).
pub fn judge_encoding<W: Workload>(format: Format, got: &[u8], reference: &[u8]) -> Verdict {
    if got == reference {
        return Verdict::Exact;
    }
    let decoded: Result<W::Owned, String> = match format {
        Format::Json => serde_json::from_slice(got).map_err(|e| e.to_string()),
        Format::Cbor => serde_cbor::from_slice(got).map_err(|e| e.to_string()),
        Format::Protobuf | Format::MinicborNative => {
            let back = if format == Format::Protobuf {
                W::Native::proto_decode(bytes::Bytes::copy_from_slice(got))
            } else {
                W::Native::mini_decode(got)
            };
            return match back {
                Ok(value) if Some(&value) == W::native().as_ref() => Verdict::Exact,
                Ok(_) => Verdict::Wrong("the output reads back as a different value".into()),
                Err(e) => Verdict::Wrong(format!("the output does not read back: {e}")),
            };
        }
    };
    match decoded {
        Ok(typed) => compare(format, &to_value(&typed), &to_value(&W::value())),
        Err(e) => Verdict::Wrong(format!("the output does not read back as {}: {e}", W::NAME)),
    }
}

#[cfg(test)]
mod tests {
    use payloads::families::{F32s, Floats};
    use payloads::kynos::{Catalog, JsonLarge};
    use payloads::reject::RejectTruncated;

    use super::*;

    #[test]
    fn a_float_off_by_a_bit_is_wrong_in_cbor_and_inexact_in_json() {
        let values = Floats::value();
        let mut nudged = values.clone();
        nudged[0] = f64::from_bits(nudged[0].to_bits() + 1);
        let cbor = judge_decode::<Floats, _>(Format::Cbor, Ok(nudged.clone()));
        assert!(matches!(cbor, Verdict::Wrong(_)), "{cbor:?}");
        let json = judge_decode::<Floats, _>(Format::Json, Ok(nudged));
        assert_eq!(json, Verdict::Inexact { floats: 1, max_ulps: 1 });
        assert_eq!(judge_decode::<Floats, _>(Format::Cbor, Ok(values)), Verdict::Exact);
    }

    #[test]
    fn a_wider_float_than_the_reference_is_the_same_value() {
        // Every f32 written as a double, by hand (serde_cbor would shorten
        // it): not the reference bytes, the same value.
        let values = F32s::value();
        let mut got = Vec::new();
        payloads::head(&mut got, 4, values.len() as u64);
        for value in values {
            got.push(0xfb);
            got.extend_from_slice(&f64::from(value).to_be_bytes());
        }
        let reference = F32s::bytes(Format::Cbor);
        assert_ne!(got, reference);
        assert_eq!(judge_encoding::<F32s>(Format::Cbor, &got, &reference), Verdict::Exact);
    }

    #[test]
    fn an_encode_that_is_not_the_value_is_wrong() {
        let reference = JsonLarge::bytes(Format::Cbor);
        let short = serde_cbor::to_vec(&Catalog {
            items: Vec::new(),
            ..JsonLarge::value()
        })
        .expect("encodes");
        assert!(matches!(
            judge_encoding::<JsonLarge>(Format::Cbor, &short, &reference),
            Verdict::Wrong(_)
        ));
        assert!(matches!(
            judge_encoding::<JsonLarge>(Format::Cbor, b"\xff", &reference),
            Verdict::Wrong(_)
        ));
    }

    #[test]
    fn a_refusal_is_judged_by_its_status() {
        let failure = |class| Failure::new(class, "x");
        assert_eq!(
            judge_decode::<RejectTruncated, Catalog>(Format::Cbor, Err(failure(Class::Eof))),
            Verdict::Exact
        );
        assert!(matches!(
            judge_decode::<RejectTruncated, Catalog>(Format::Cbor, Err(failure(Class::Data))),
            Verdict::Wrong(_)
        ));
        assert!(matches!(
            judge_decode::<JsonLarge, Catalog>(Format::Cbor, Err(Failure::unsupported())),
            Verdict::Rejected(_)
        ));
    }

    #[test]
    fn native_values_are_compared_whole() {
        assert_eq!(judge_native::<JsonLarge>(Ok(JsonLarge::value())), Verdict::Exact);
        let mut other = JsonLarge::value();
        other.total += 1;
        assert!(matches!(judge_native::<JsonLarge>(Ok(other)), Verdict::Wrong(_)));
    }
}
