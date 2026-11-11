//! Section `encode_diff`: every workload, encoded and decoded, against the
//! serde_cbor oracle and the workload's own value.
//!
//! Encode side: `encode` and `encode_into` (into a buffer holding stale
//! bytes, which must be cleared) must produce the oracle's bytes (preferred
//! serialization), or bytes the oracle reads into the workload's type as the
//! same value (a float written wider than it needs is the same float): a
//! non-gating difference, listed with the first differing offset. Anything
//! else gates, as does failing to encode a value. Past the oracle's 128
//! levels the output is read back by the library itself instead.
//! Decode side: the oracle's bytes decoded owned and borrowed, through both
//! arrivals, must be the workload's value exactly (for `reject-*`, refused
//! with its status), except where the library refuses on its own depth limit
//! (that policy is `depth`'s to report).

use std::marker::PhantomData;

use codecs::{Backend, Format};
use payloads::Workload;
use serde_json::{Value, json};

use crate::outcome::{ARRIVALS, BorrowedForm, Outcome, Typed, borrowed, guarded, owned, same_value, to_value};
use crate::record::Section;

/// Bytes either side of the first difference shown in the report.
pub const WINDOW: usize = 40;

/// Where two encodings first differ, with a window of each.
#[must_use]
pub fn first_difference(reference: &[u8], backend: &[u8]) -> Value {
    let offset = reference.iter().zip(backend).take_while(|(a, b)| a == b).count();
    let window = |bytes: &[u8]| {
        let start = offset.saturating_sub(WINDOW / 2).min(bytes.len());
        let end = (start + WINDOW).min(bytes.len());
        hex::encode(&bytes[start..end])
    };
    json!({
        "offset": offset,
        "reference_len": reference.len(),
        "backend_len": backend.len(),
        "reference_window": window(reference),
        "backend_window": window(backend),
    })
}

struct Visit<'s, B> {
    section: &'s mut Section,
    backend: PhantomData<B>,
}

/// Whether `reader` reads both encodings into `W::Owned` as the same value.
fn typed_equal<W: Workload>(reader: impl Fn(&[u8]) -> Option<W::Owned>, reference: &[u8], backend: &[u8]) -> bool {
    match (reader(reference), reader(backend)) {
        (Some(r), Some(b)) => same_value(&to_value(&r), &to_value(&b)),
        _ => false,
    }
}

fn compare_encoding<B: Backend, W: Workload>(
    section: &mut Section,
    case: &str,
    reference: &[u8],
    backend: Result<Result<Vec<u8>, codecs::Failure>, String>,
) {
    section.case();
    let backend = match backend {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(failure)) => {
            section.differ(case, json!("encoded"), json!({ "error": failure.to_string() }));
            section.fail(case, format!("encode failed: {failure}"));
            return;
        }
        Err(panic) => {
            section.differ(case, json!("encoded"), json!({ "panic": panic }));
            section.fail(case, format!("encode panicked: {panic}"));
            return;
        }
    };
    if backend == reference {
        return;
    }
    let difference = first_difference(reference, &backend);
    let oracle = |bytes: &[u8]| serde_cbor::from_slice::<W::Owned>(bytes).ok();
    let itself = |bytes: &[u8]| B::decode::<W::Owned>(bytes::Bytes::copy_from_slice(bytes)).ok();
    if typed_equal::<W>(oracle, reference, &backend) {
        section.differ(case, json!("the same value"), difference);
    } else if oracle(reference).is_none() && typed_equal::<W>(itself, reference, &backend) {
        section.differ(
            case,
            json!("the same value, read back by the library (past the oracle's depth)"),
            difference,
        );
    } else {
        section.differ(case, json!("another value"), difference);
        section.fail(case, "the encoding holds another value than the oracle's");
    }
}

impl<B: Backend> payloads::Visit for Visit<'_, B> {
    type Output = ();

    fn visit<W: Workload>(self) {
        let section = self.section;
        let value = W::value();
        // A body that must be refused encodes no value; its value is the one
        // it was damaged from, whose encode another workload checks.
        if B::ENCODES && W::EXPECT == payloads::Expect::Value {
            let reference = payloads::cbor(&value);
            compare_encoding::<B, W>(
                section,
                &format!("{}#encode", W::NAME),
                &reference,
                guarded(|| B::encode(&value)),
            );
            let into = guarded(|| {
                let mut out = b"stale bytes the encoder must clear".to_vec();
                B::encode_into(&value, &mut out).map(|()| out)
            });
            compare_encoding::<B, W>(section, &format!("{}#encode_into", W::NAME), &reference, into);
        }
        let bytes = W::bytes(Format::Cbor);
        let expected = match W::EXPECT {
            payloads::Expect::Value => Some(to_value(&value)),
            payloads::Expect::Reject(_) => None,
        };
        for arrival in ARRIVALS {
            let paths = [
                ("owned", owned::<B, Typed<W::Owned>>(&bytes, arrival)),
                ("borrowed", borrowed::<B, BorrowedForm<W>>(&bytes, arrival)),
            ];
            for (path, outcome) in paths {
                let case = format!("{}#decode/{path}#{}", W::NAME, arrival.name());
                section.case();
                let right = match (&expected, &outcome, W::EXPECT) {
                    (Some(value), Outcome::Accept(got), _) => same_value(value, got),
                    (None, Outcome::Reject(failure), payloads::Expect::Reject(status)) => failure.status() == status,
                    _ => false,
                };
                if right {
                    continue;
                }
                let want = expected
                    .as_ref()
                    .map_or_else(|| json!(format!("{:?}", W::EXPECT)), crate::outcome::abbreviate);
                section.differ(&case, want, outcome.describe());
                match &outcome {
                    Outcome::Panic(message) => section.fail(&case, format!("panicked: {message}")),
                    Outcome::Reject(failure) if failure.class == codecs::Class::Unsupported => {
                        section.note(format!("{case}: the library has no such entry point"));
                    }
                    Outcome::Reject(_) if W::NAME.starts_with("nested-") && W::NAME != "nested-100" => {
                        section.note(format!("{case}: refused past the library's depth limit (see depth)"));
                    }
                    Outcome::Reject(_) if expected.is_some() => section.fail(&case, "the oracle's encoding refused"),
                    _ => section.fail(&case, "decoded to another value, or answered another status"),
                }
            }
        }
    }
}

/// Run the section.
#[must_use]
pub fn run<B: Backend>() -> Section {
    let mut section = Section::new("encode_diff");
    if !B::ENCODES {
        section.note("decode-only backend: encode checks skipped, decode checks run");
    }
    for name in payloads::ALL {
        payloads::dispatch(
            name,
            Visit::<B> {
                section: &mut section,
                backend: PhantomData,
            },
        );
    }
    section
}

#[cfg(test)]
mod tests {
    use codecs::backend::serde_cbor::SerdeCbor;
    use payloads::families::F32s;

    use super::*;

    /// `values` written as doubles, by hand: what an encoder that never
    /// narrows writes.
    fn widened(values: &[f32]) -> Vec<u8> {
        let mut out = Vec::new();
        payloads::head(&mut out, 4, values.len() as u64);
        for value in values {
            out.push(0xfb);
            out.extend_from_slice(&f64::from(*value).to_be_bytes());
        }
        out
    }

    fn judged(output: Vec<u8>) -> (Value, usize) {
        let mut section = Section::new("encode_diff");
        compare_encoding::<SerdeCbor, F32s>(&mut section, "f32s#encode", &F32s::bytes(Format::Cbor), Ok(Ok(output)));
        let failures = section.failure_count();
        (section.finish().0, failures)
    }

    #[test]
    fn f32s_written_wide_differ_only_in_bytes() {
        let (record, failures) = judged(widened(&F32s::value()));
        assert_eq!(failures, 0, "{record}");
        assert_eq!(record["differences"][0]["reference"], "the same value");
    }

    #[test]
    fn a_different_f32_is_another_value() {
        let mut values = F32s::value();
        values[3] = f32::from_bits(values[3].to_bits() + 1);
        let (record, failures) = judged(widened(&values));
        assert_eq!(failures, 1, "{record}");
    }
}
