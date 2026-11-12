//! What a divergence is, shared by every target, and which backend a run
//! fuzzes.
//!
//! A build compiles the baselines, the serde_cbor oracle and one candidate
//! crate. Each run fuzzes the candidate named by `FUZZ_BACKEND`; unset, the
//! first CBOR candidate compiled, and with none, the oracle against itself
//! (the harness's own check). A divergence panics, which libFuzzer records as
//! a crash with its input.
//!
//! # The rule
//!
//! A divergence is a silent difference, as the conformance suite defines it,
//! judged by RFC 8949's well-formedness through `faithful::well_formed`,
//! which shares no code with any candidate, and by the oracle's value:
//!
//! 1. An ill-formed body (RFC 8949 section 5.1, invalid UTF-8 included) must
//!    be refused. Accepting it diverges. One exception is decided by rule,
//!    not by message: a body whose only fault is bytes after one complete
//!    item ([`faithful::Rule::Trailing`]) is the documented behaviour of a
//!    backend whose [`codecs::Backend::TRAILING`] is
//!    [`codecs::Trailing::Ignored`]; it is counted
//!    ([`Tally::TrailingIgnored`]), not a divergence. Conformance reports
//!    that policy.
//! 2. A body the candidate and the oracle both accept must decode to the
//!    same value, owned and borrowed, floats by bits and tags by number.
//!
//! Everything else is a loud answer, which conformance judges case by case:
//! an ill-formed body refused as a valid item of the wrong shape (422, not
//! 400), which section `wellformed` gates; a well-formed body the candidate
//! refuses (a depth limit, a validity rule, a type it cannot produce)
//! whatever the oracle does; and a status class that differs from the
//! oracle's on a well-formed body. A refusal is not a silent difference, and
//! panicking on the wrong class would spend a run reproducing the one
//! finding `wellformed` already gates (every candidate answers some
//! ill-formed bodies 422) instead of searching for the silent ones and for
//! memory errors. Those answers are counted ([`Tally`]): each count is
//! printed to stderr as a `fuzz-tally` line when it reaches a power of two,
//! and, where [`TALLY_DIR_VAR`] names a directory, written there per
//! process, so the run's record says how often each happened even from
//! fork-mode workers, whose stderr libFuzzer does not relay. The worked codec's own
//! data-model rules (`faithful::scan`'s 422s) are no library's duty, so they
//! draw no line here.

use std::marker::PhantomData;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use codecs::{Backend, Class, Failure, Trailing, Visit};

/// The environment variable naming the backend a run fuzzes.
pub const BACKEND_VAR: &str = "FUZZ_BACKEND";

/// The backend this run fuzzes, resolved once.
///
/// # Panics
///
/// If `FUZZ_BACKEND` names a backend this build did not compile, so a
/// misconfigured run fails at its first input rather than fuzzing another
/// backend under that name.
#[must_use]
pub fn candidate() -> &'static str {
    static NAME: OnceLock<&'static str> = OnceLock::new();
    NAME.get_or_init(|| {
        let compiled = codecs::compiled();
        match std::env::var(BACKEND_VAR) {
            Ok(name) => compiled
                .iter()
                .copied()
                .find(|c| *c == name)
                .unwrap_or_else(|| panic!("{BACKEND_VAR}={name} is not compiled into this build: {compiled:?}")),
            Err(_) => codecs::cbor_candidates().first().copied().unwrap_or("serde_cbor"),
        }
    })
}

/// One target's check, generic over the backend it runs against.
pub trait Check {
    /// Check `data` against `B`, panicking on a divergence.
    fn check<B: Backend>(data: &[u8]);
}

struct Run<'a, C>(&'a [u8], PhantomData<C>);

impl<C: Check> Visit for Run<'_, C> {
    type Output = ();
    fn visit<B: Backend>(self) {
        C::check::<B>(self.0);
    }
}

/// Run `C`'s check on `data` against [`candidate`].
pub fn run<C: Check>(data: &[u8]) {
    codecs::dispatch(candidate(), Run::<C>(data, PhantomData));
}

/// The environment variable naming the directory each process writes its
/// tallies to (one file per process id, rewritten whenever a count reaches
/// a power of two).
pub const TALLY_DIR_VAR: &str = "FUZZ_TALLY_DIR";

/// The loud answers and expected behaviours a run counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tally {
    /// Bytes after a complete item accepted by a backend that documents
    /// ignoring them (rule 1's exception).
    TrailingIgnored,
    /// An ill-formed body refused as a valid item of the wrong shape: 422
    /// where Kynos's taxonomy says 400. Section `wellformed` gates it.
    IllFormedData,
    /// A well-formed body the candidate refused while the oracle accepted.
    RefusedWellFormed,
    /// A well-formed body both refused, with different status classes.
    OtherClass,
    /// A borrowed decode refused where the owned one was accepted: the
    /// candidate does not borrow this value.
    NotBorrowed,
    /// A value holding a null encoded by a backend that writes unit (which
    /// `Value::Null` serializes as) as an empty array, not `null`.
    UnitNotNull,
}

impl Tally {
    const ALL: [Self; 6] = [
        Self::TrailingIgnored,
        Self::IllFormedData,
        Self::RefusedWellFormed,
        Self::OtherClass,
        Self::NotBorrowed,
        Self::UnitNotNull,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::TrailingIgnored => "trailing-ignored",
            Self::IllFormedData => "ill-formed-422",
            Self::RefusedWellFormed => "refused-well-formed",
            Self::OtherClass => "other-class",
            Self::NotBorrowed => "not-borrowed",
            Self::UnitNotNull => "unit-not-null",
        }
    }
}

static TALLIES: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];

/// Count one `tally`. Each time the count reaches a power of two (so a long
/// run's log stays short and still says how often) it is printed to stderr,
/// and every count this process holds is written to its file under
/// [`TALLY_DIR_VAR`], so the file is never more than a factor of two behind.
pub fn tally(tally: Tally) {
    let index = Tally::ALL.iter().position(|t| *t == tally).unwrap_or(0);
    let count = TALLIES[index].fetch_add(1, Ordering::Relaxed) + 1;
    if count.is_power_of_two() {
        eprintln!("fuzz-tally {} {count} backend={}", tally.name(), candidate());
        if let Some(dir) = std::env::var_os(TALLY_DIR_VAR) {
            let counts: String = Tally::ALL
                .iter()
                .zip(&TALLIES)
                .map(|(t, n)| format!("{} {}\n", t.name(), n.load(Ordering::Relaxed)))
                .collect();
            // A lost snapshot under-counts; it cannot fail the check.
            let _ = std::fs::write(std::path::Path::new(&dir).join(std::process::id().to_string()), counts);
        }
    }
}

/// How `data`'s form judges an answer (rule 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// One well-formed item.
    WellFormed,
    /// One well-formed item and bytes after it, which `B` documents
    /// ignoring.
    TrailingIgnored,
    /// Ill-formed: it must be refused with 400.
    IllFormed(faithful::Refusal),
}

/// `data`'s [`Form`] for backend `B`.
#[must_use]
pub fn form<B: Backend>(data: &[u8]) -> Form {
    match faithful::well_formed(data) {
        Ok(()) => Form::WellFormed,
        Err(refusal) if refusal.rule == faithful::Rule::Trailing && B::TRAILING == Trailing::Ignored => {
            Form::TrailingIgnored
        }
        Err(refusal) => Form::IllFormed(refusal),
    }
}

/// Rule 1: panic if the candidate's answer to `data` is not one its form
/// allows. `oracle` is the oracle's answer to the same decode, for the
/// tallies.
///
/// # Panics
///
/// On a divergence.
pub fn judge_form<B: Backend, T>(
    data: &[u8],
    what: &str,
    answer: &Result<T, Failure>,
    oracle: Option<&Result<T, Failure>>,
) {
    match (form::<B>(data), answer) {
        (Form::IllFormed(refusal), Ok(_)) => panic!("{what}: accepted an ill-formed body ({refusal})"),
        (Form::IllFormed(_), Err(failure)) if failure.class == Class::Data => tally(Tally::IllFormedData),
        (Form::TrailingIgnored, Ok(_)) => tally(Tally::TrailingIgnored),
        (Form::WellFormed, Err(failure)) => match oracle {
            Some(Ok(_)) => tally(Tally::RefusedWellFormed),
            Some(Err(theirs)) if theirs.status() != failure.status() => tally(Tally::OtherClass),
            _ => {}
        },
        _ => {}
    }
}

/// Whether two decoded values are the same, floats by bits (any NaN equal to
/// any NaN: RFC 8949 leaves NaN payloads to the application) and tags by
/// number: a tag on one side only, or another tag number, differs.
#[must_use]
pub fn same(a: &serde_cbor::Value, b: &serde_cbor::Value) -> bool {
    same_in(a, b, &[])
}

/// One bignum in a body: tag 2 or 3 and its byte string's content.
pub type Bignum = (u64, Vec<u8>);

/// [`same`], for a candidate's value `a` and the oracle's `b` decoded from a
/// body holding `bignums` (see [`bignums`]).
///
/// The oracle is built without serde_cbor's `tags` feature, so it drops every
/// tag and reads a bignum as its byte string. A library that reads tags 2
/// and 3 as RFC 8949 section 3.4.3 says (`n` and `-1 - n`, `n` the bytes as
/// a big-endian unsigned integer) gives an integer there instead. That
/// integer matches the oracle's bytes only when the body holds that bignum
/// with that tag and the integer is its value, so a wrong value, sign or
/// width still differs.
#[must_use]
pub fn same_in(a: &serde_cbor::Value, b: &serde_cbor::Value, bignums: &[Bignum]) -> bool {
    use serde_cbor::Value;
    let same = |a, b| same_in(a, b, bignums);
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits() || x.is_nan() && y.is_nan(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y)),
        // Pairwise in order first; a bignum key read as an integer sorts
        // elsewhere in the candidate's map than its bytes do in the oracle's,
        // so failing that, each entry must match one in the other map.
        (Value::Map(x), Value::Map(y)) => {
            x.len() == y.len()
                && (x.iter().zip(y).all(|((kx, vx), (ky, vy))| same(kx, ky) && same(vx, vy))
                    || !bignums.is_empty()
                        && x.iter()
                            .all(|(kx, vx)| y.iter().any(|(ky, vy)| same(kx, ky) && same(vx, vy))))
        }
        (Value::Tag(m, x), Value::Tag(n, y)) => m == n && same(x, y),
        (Value::Integer(v), Value::Bytes(bytes)) => bignums
            .iter()
            .any(|(tag, content)| content == bytes && bignum_value(*tag, content) == Some(*v)),
        _ => a == b,
    }
}

/// A bignum's value (RFC 8949 section 3.4.3), where it fits an `i128`.
fn bignum_value(tag: u64, content: &[u8]) -> Option<i128> {
    let n = content
        .iter()
        .try_fold(0i128, |n, &byte| n.checked_mul(256)?.checked_add(i128::from(byte)))?;
    match tag {
        2 => Some(n),
        3 => Some(-1 - n),
        _ => None,
    }
}

/// Every bignum in `body`'s first item: each tag 2 or 3 whose content is a
/// byte string (definite, or indefinite with its chunks joined). It reads
/// heads in order, which for a well-formed body visits every item; on a
/// head it cannot read it stops, with what it found so far.
#[must_use]
pub fn bignums(body: &[u8]) -> Vec<Bignum> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some((major, minor, arg, next)) = head(body, at) {
        at = next;
        match (major, minor) {
            // A definite byte or text string: skip its content.
            (2 | 3, 0..=27) => at = at.saturating_add(usize::try_from(arg).unwrap_or(usize::MAX)),
            (6, _) if arg == 2 || arg == 3 => {
                if let Some((content, next)) = byte_string(body, at) {
                    found.push((arg, content));
                    at = next;
                }
            }
            _ => {}
        }
        if at > body.len() {
            break;
        }
    }
    found
}

/// The head at `at`: major type, additional information, argument (the
/// length, value or tag number; a float's bits), and where its content or
/// the next head begins.
fn head(body: &[u8], at: usize) -> Option<(u8, u8, u64, usize)> {
    let initial = *body.get(at)?;
    let (major, minor) = (initial >> 5, initial & 0x1f);
    let width = match minor {
        0..=23 => return Some((major, minor, u64::from(minor), at + 1)),
        24 => 1,
        25 => 2,
        26 => 4,
        27 => 8,
        31 => return Some((major, minor, 0, at + 1)),
        _ => return None,
    };
    let bytes = body.get(at + 1..at + 1 + width)?;
    let arg = bytes.iter().fold(0u64, |n, &b| (n << 8) | u64::from(b));
    Some((major, minor, arg, at + 1 + width))
}

/// The byte string at `at`, and where it ends.
fn byte_string(body: &[u8], at: usize) -> Option<(Vec<u8>, usize)> {
    let (major, minor, arg, mut at) = head(body, at)?;
    if major != 2 {
        return None;
    }
    if minor != 31 {
        let end = at.checked_add(usize::try_from(arg).ok()?)?;
        return Some((body.get(at..end)?.to_vec(), end));
    }
    let mut content = Vec::new();
    loop {
        if *body.get(at)? == 0xff {
            return Some((content, at + 1));
        }
        let (chunk, next) = byte_string(body, at)?;
        content.extend(chunk);
        at = next;
    }
}

#[cfg(test)]
mod tests {
    use serde_cbor::Value;

    use super::{bignums, same, same_in};

    /// The fuzzer's first find: tag 3 on an empty byte string is -1.
    #[test]
    fn a_bignum_read_as_its_value_matches_the_oracles_bytes() {
        let body = [0xc3, 0x40];
        let found = bignums(&body);
        assert_eq!(found, vec![(3, vec![])]);
        let bytes = Value::Bytes(vec![]);
        assert!(same_in(&Value::Integer(-1), &bytes, &found));
        // The wrong sign, the wrong value, or no bignum in the body differs.
        assert!(!same_in(&Value::Integer(0), &bytes, &found));
        assert!(!same_in(&Value::Integer(-1), &bytes, &[(2, vec![])]));
        assert!(!same(&Value::Integer(-1), &bytes));
        // Nested, indefinite, and as a map key that sorts elsewhere.
        let body = [
            0xa1, 0xc2, 0x5f, 0x41, 0x01, 0x41, 0x00, 0xff, 0x82, 0xc3, 0x41, 0x01, 0x00,
        ];
        let found = bignums(&body);
        assert_eq!(found, vec![(2, vec![1, 0]), (3, vec![1])]);
        let map = |key, value| Value::Map(std::iter::once((key, value)).collect());
        let candidate = map(
            Value::Integer(256),
            Value::Array(vec![Value::Integer(-2), Value::Integer(0)]),
        );
        let oracle = map(
            Value::Bytes(vec![1, 0]),
            Value::Array(vec![Value::Bytes(vec![1]), Value::Integer(0)]),
        );
        assert!(same_in(&candidate, &oracle, &found));
        // A byte string's content is skipped, not read as heads.
        assert!(bignums(&[0x42, 0xc2, 0x41]).is_empty());
    }

    #[test]
    fn tags_are_compared_by_number() {
        let one = Value::Integer(1);
        let tagged = |n| Value::Tag(n, Box::new(one.clone()));
        assert!(same(&tagged(2), &tagged(2)));
        assert!(!same(&tagged(2), &tagged(3)));
        assert!(!same(&tagged(2), &one));
        assert!(!same(&one, &tagged(2)));
    }

    #[test]
    fn floats_are_compared_by_bits() {
        assert!(!same(&Value::Float(0.0), &Value::Float(-0.0)));
        assert!(same(&Value::Float(f64::NAN), &Value::Float(-f64::NAN)));
    }
}
