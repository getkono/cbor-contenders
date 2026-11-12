//! A slice of the suite, for Miri to look for undefined behaviour in the
//! candidates' decoders and encoders.
//!
//! `MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test -p conformance
//! --test miri --features <F>`. Nothing about agreement is asserted: that is
//! the suite's job, and here only Miri's verdict matters.
//!
//! One test per section, so the test runner's own output says which section
//! ran and which did not. A section Miri cannot run is a test ignored under
//! Miri with the reason, which the runner prints beside it: `depth` and
//! `deterministic` re-execute the `conform` binary, and Miri runs no
//! subprocess. `status` and `encode_diff` run on `json-small` (and the wrong
//! shape) only, since every truncation of the larger bodies would take Miri
//! days. `lengths` runs under the counting allocator, installed here as it
//! is in `conform`. Everything also runs under plain `cargo test`, so it
//! keeps compiling and running.

use codecs::{Backend, Visit};
use conformance::record::Section;
use conformance::{
    appendix, attributes, depth, deterministic, encode_diff, floats, integers, lengths, model, status, strings,
    structs, wellformed,
};
use payloads::Workload;

#[global_allocator]
static ALLOCATOR: alloc_count::Counting = alloc_count::Counting;

/// Random floats per backend: enough to cross every number path.
const SAMPLES: usize = 1_000;

/// Run `section` for every candidate this build has.
fn each(section: fn(&str) -> Option<Section>) {
    for name in codecs::cbor_candidates() {
        let _ = section(name);
    }
}

/// A section as a function of the backend's name.
macro_rules! by_name {
    (|$b:ident| $run:expr) => {{
        struct Run;
        impl Visit for Run {
            type Output = Section;
            fn visit<$b: Backend>(self) -> Section {
                $run
            }
        }
        |name: &str| codecs::dispatch(name, Run)
    }};
}

#[test]
fn appendix_a() {
    let rows = appendix::load(&appendix::default_dir()).expect("the transcription verifies");
    for name in codecs::cbor_candidates() {
        struct Run<'a>(&'a [appendix::Row]);
        impl Visit for Run<'_> {
            type Output = Section;
            fn visit<B: Backend>(self) -> Section {
                appendix::run::<B>(self.0)
            }
        }
        let _ = codecs::dispatch(name, Run(&rows));
    }
}

#[test]
fn wellformed() {
    each(by_name!(|B| wellformed::run::<B>()));
}

#[test]
fn strings() {
    each(by_name!(|B| strings::run::<B>()));
}

#[test]
fn structs() {
    each(by_name!(|B| structs::run::<B>()));
}

#[test]
fn lengths() {
    each(by_name!(|B| lengths::run::<B>()));
}

#[test]
fn integers() {
    each(by_name!(|B| integers::run::<B>()));
}

#[test]
fn floats() {
    each(by_name!(|B| floats::run::<B>(SAMPLES)));
}

#[test]
fn attributes() {
    each(by_name!(|B| attributes::run::<B>()));
}

#[test]
fn model() {
    each(by_name!(|B| model::run::<B>()));
}

#[test]
fn status_on_json_small() {
    each(by_name!(|B| status::small::<B>()));
}

#[test]
fn encode_diff_on_json_small() {
    each(by_name!(|B| encode_diff::run_on::<B>(&[
        payloads::kynos::JsonSmall::NAME
    ])));
}

/// Without a binary to re-execute the section only notes that it skipped.
#[test]
#[cfg_attr(
    miri,
    ignore = "unsupported: depth re-executes the conform binary, and Miri runs no subprocess"
)]
fn depth() {
    each(by_name!(|B| depth::run::<B>(None)));
}

/// The in-process half runs; the cross-process half needs a subprocess.
#[test]
#[cfg_attr(
    miri,
    ignore = "unsupported: deterministic encodes every workload, which Miri would take days over, and re-executes the conform binary"
)]
fn deterministic() {
    each(by_name!(|B| deterministic::run::<B>(None)));
}
