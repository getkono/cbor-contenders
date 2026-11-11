//! A small slice of the suite, for Miri to look for undefined behaviour in
//! the candidates' decoders and encoders.
//!
//! `MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test -p conformance
//! --test miri --features <F>`. Nothing about agreement is asserted: that is
//! the suite's job, and here only Miri's verdict matters. No subprocesses and
//! no custom stacks, which Miri cannot run. Also runs under plain
//! `cargo test`, so it keeps compiling.

use codecs::{Backend, Visit};
use conformance::{appendix, floats, integers, strings, wellformed};

/// Random floats per backend: enough to cross every number path.
const SAMPLES: usize = 1_000;

struct Smoke<'a>(&'a [appendix::Row]);

impl Visit for Smoke<'_> {
    type Output = ();

    fn visit<B: Backend>(self) {
        let _ = appendix::run::<B>(self.0);
        let _ = wellformed::run::<B>();
        let _ = strings::run::<B>();
        let _ = integers::run::<B>();
        let _ = floats::run::<B>(SAMPLES);
    }
}

#[test]
fn every_candidate_runs_the_smoke_slice() {
    let rows = appendix::load(&appendix::default_dir()).expect("the transcription verifies");
    for name in codecs::cbor_candidates() {
        codecs::dispatch(name, Smoke(&rows));
    }
}
