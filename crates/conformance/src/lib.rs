//! Conformance of every compiled CBOR candidate: against RFC 8949's own text
//! where the RFC answers, against serde_json on the same value written as
//! JSON where serde's derive decides the answer, and differentially against
//! the serde_cbor oracle and the other candidates everywhere.
//!
//! Each section module proves one property a server relies on when it reads
//! and writes CBOR bodies: that the Appendix A examples decode to their
//! values, ill-formed and invalid bodies are refused with 400, strings are
//! UTF-8, nesting cannot crash the process, a declared length cannot make it
//! allocate, integers and floats keep their values, encoding is stable, the
//! serde attributes behave, and every workload round-trips. Section `model`
//! records what each library does with the CBOR values the JSON data model
//! has no place for (#989 section 5), and which the worked codec refuses.
//!
//! A backend misbehaving is a finding, never a crash of the suite: every
//! decode and encode runs under `catch_unwind`, depth probes run in a
//! subprocess, and the `conform` binary runs each backend's suite in its own
//! process.
//!
//! *Silent* differences gate: a body accepted that the RFC refuses, a value
//! other than the input's, a status of the other class, a crash, an
//! allocation of a declared length. Answers RFC 8949 leaves to the
//! implementation (duplicate map keys, tags, bignums, simple values, depth
//! limits, encoding choices) are recorded, and each section says which.

pub mod appendix;
pub mod attributes;
pub mod cases;
pub mod depth;
pub mod deterministic;
pub mod encode_diff;
pub mod floats;
pub mod integers;
pub mod lengths;
pub mod model;
pub mod outcome;
pub mod record;
pub mod status;
pub mod strings;
pub mod structs;
pub mod wellformed;

use std::path::PathBuf;

use codecs::Backend;
use serde_json::{Map, Value, json};

use crate::record::Section;

/// Version of the report's shape.
pub const SCHEMA: u32 = 1;

/// Stack for the thread a backend's suite runs on: deep enough that the
/// suite's own recursion (serializing and comparing `nested-10000`) is never
/// what overflows. Reserved, not committed.
pub const SUITE_STACK: usize = 512 << 20;

/// What one run needs besides the backend.
#[derive(Debug, Clone)]
pub struct Suite {
    appendix: Vec<appendix::Row>,
    probe: Option<PathBuf>,
    float_samples: usize,
}

impl Suite {
    /// A suite over the verified Appendix A rows, with no subprocess probes
    /// and [`floats::SAMPLES`] random floats.
    #[must_use]
    pub fn new(appendix: Vec<appendix::Row>) -> Self {
        Self {
            appendix,
            probe: None,
            float_samples: floats::SAMPLES,
        }
    }

    /// Run depth and stability probes by re-executing `exe` (the `conform`
    /// binary).
    #[must_use]
    pub fn with_probe(mut self, exe: PathBuf) -> Self {
        self.probe = Some(exe);
        self
    }

    /// Check `samples` random floats instead of [`floats::SAMPLES`].
    #[must_use]
    pub fn with_float_samples(mut self, samples: usize) -> Self {
        self.float_samples = samples;
        self
    }

    /// Every section for `B`, as the report's backend entry.
    ///
    /// Runs on a thread with a [`SUITE_STACK`] stack, so it is safe to call
    /// from a test thread.
    #[must_use]
    pub fn run_backend<B: Backend>(&self) -> Value {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name(format!("conform-{}", B::NAME))
                .stack_size(SUITE_STACK)
                .spawn_scoped(scope, || {
                    let mut entry = record::backend_entry(self.sections::<B>());
                    entry["trailing"] = json!(B::TRAILING);
                    entry
                })
                .map_or_else(
                    |error| harness_failure(&format!("could not spawn the suite thread: {error}")),
                    |handle| handle.join().unwrap_or_else(|_| crashed("the suite itself panicked")),
                )
        })
    }

    fn sections<B: Backend>(&self) -> Vec<Section> {
        vec![
            appendix::run::<B>(&self.appendix),
            wellformed::run::<B>(),
            strings::run::<B>(),
            structs::run::<B>(),
            depth::run::<B>(self.probe.as_deref()),
            lengths::run::<B>(),
            integers::run::<B>(),
            floats::run::<B>(self.float_samples),
            deterministic::run::<B>(self.probe.as_deref()),
            attributes::run::<B>(),
            model::run::<B>(),
            status::run::<B>(),
            encode_diff::run::<B>(),
        ]
    }
}

/// A backend entry for a suite the backend took down: a finding against the
/// library, which gates.
#[must_use]
pub fn crashed(detail: &str) -> Value {
    json!({
        "gate": "fail",
        "gating_failures": [{ "section": "process", "case": "suite", "detail": detail }],
        "sections": {},
    })
}

/// A backend entry for a suite the harness could not run or read: no evidence
/// about the library either way, so it carries an `error` the report reads as
/// a gap, and no gate failure.
#[must_use]
pub fn harness_failure(detail: &str) -> Value {
    json!({
        "gate": "error",
        "error": detail,
        "gating_failures": [],
        "sections": {},
    })
}

/// Target features this binary was compiled with.
#[must_use]
pub fn target_features() -> Vec<&'static str> {
    let mut features = Vec::new();
    macro_rules! probe {
        ($($f:literal),*) => { $( if cfg!(target_feature = $f) { features.push($f); } )* };
    }
    probe!(
        "sse2",
        "ssse3",
        "sse4.1",
        "sse4.2",
        "popcnt",
        "avx",
        "avx2",
        "bmi1",
        "bmi2",
        "fma",
        "lzcnt",
        "pclmulqdq",
        "avx512f",
        "avx512bw"
    );
    features
}

/// The report around `backends`, each keyed by name.
#[must_use]
pub fn report(backends: Map<String, Value>) -> Value {
    let mut report = json!({
        "schema": SCHEMA,
        "reference": codecs::backend::serde_cbor::SerdeCbor::NAME,
        "target_features": target_features(),
    });
    report["backends"] = Value::Object(backends);
    report
}

/// The report for a run that could not start, such as a corpus that failed
/// verification.
#[must_use]
pub fn error_report(error: &str) -> Value {
    let mut report = report(Map::new());
    report["error"] = json!(error);
    report
}
