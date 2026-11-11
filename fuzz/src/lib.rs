//! What a divergence is, shared by every target, and which backend a run
//! fuzzes.
//!
//! A build compiles the baselines, the serde_cbor oracle and one candidate
//! crate. Each run fuzzes the candidate named by `FUZZ_BACKEND`; unset, the
//! first CBOR candidate compiled, and with none, the oracle against itself
//! (the harness's own check). A divergence panics, which libFuzzer records as
//! a crash with its input.
//!
//! A divergence is a silent difference, as the conformance suite defines it:
//! an ill-formed body accepted (RFC 8949 section 5.1, by
//! `faithful::well_formed`, which shares no code with any candidate), or a
//! body both the candidate and the oracle accept decoded to different
//! values. A refusal of a well-formed body is a loud answer, which the
//! conformance suite records; it is not a divergence here.

use std::marker::PhantomData;
use std::sync::OnceLock;

use codecs::{Backend, Visit};

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

/// Panic if `accepted` and `data` is not one well-formed CBOR item.
pub fn refuse_ill_formed(data: &[u8], what: &str, accepted: bool) {
    if accepted {
        if let Err(refusal) = faithful::well_formed(data) {
            panic!("{what}: accepted an ill-formed body ({refusal})");
        }
    }
}

/// Whether two decoded values are the same, floats by bits (any NaN equal to
/// any NaN: RFC 8949 leaves NaN payloads to the application).
#[must_use]
pub fn same(a: &serde_cbor::Value, b: &serde_cbor::Value) -> bool {
    use serde_cbor::Value;
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits() || x.is_nan() && y.is_nan(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y)),
        (Value::Map(x), Value::Map(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|((kx, vx), (ky, vy))| same(kx, ky) && same(vx, vy))
        }
        (Value::Tag(_, x), y) | (y, Value::Tag(_, x)) => same(x, y),
        _ => a == b,
    }
}
