//! The build matrix: which backend crates are built how.
//!
//! A *set* is one cargo feature set: at most one candidate crate beside the
//! two baselines (serde_json, prost) and the oracle (serde_cbor, never
//! measured), so no candidate can change another's features through
//! unification. A *variant* is one `-C target-cpu`. Every set is built at
//! every variant, the baseline included: a comparison at unequal
//! `target-cpu` is not a comparison.

/// One cargo feature set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Set {
    /// Stable identifier, also the target directory name.
    pub name: &'static str,
    /// Features passed to `cell`, `conformance`, `e2e`, `size-fixture`.
    pub features: &'static str,
    /// Backends this set measures (the baselines are measured only in the
    /// baseline set).
    pub backends: &'static [&'static str],
    /// The crates whose cost this set weighs (`size`, `compile-time`,
    /// `msrv`), each with the `size-fixture` feature that selects it.
    pub costs: &'static [(&'static str, &'static str)],
}

/// Every set, baseline first.
pub const SETS: &[Set] = &[
    Set {
        name: "baseline",
        features: "",
        backends: &["serde_json", "prost"],
        costs: &[("serde_json", ""), ("prost", "prost")],
    },
    Set {
        name: "ciborium",
        features: "ciborium",
        backends: &["ciborium"],
        costs: &[("ciborium", "ciborium")],
    },
    Set {
        name: "minicbor-serde",
        features: "minicbor-serde",
        backends: &["minicbor-serde"],
        costs: &[("minicbor-serde", "minicbor-serde")],
    },
    Set {
        name: "minicbor",
        features: "minicbor",
        backends: &["minicbor"],
        costs: &[("minicbor", "minicbor")],
    },
    Set {
        name: "cbor4ii",
        features: "cbor4ii",
        backends: &["cbor4ii"],
        costs: &[("cbor4ii", "cbor4ii")],
    },
    Set {
        name: "cbor2",
        features: "cbor2",
        backends: &["cbor2"],
        costs: &[("cbor2", "cbor2")],
    },
    Set {
        name: "c2pa_cbor",
        features: "c2pa_cbor",
        backends: &["c2pa_cbor"],
        costs: &[("c2pa_cbor", "c2pa_cbor")],
    },
];

/// The crates every set shares, whose resolved features must not move from
/// the baseline build's (`build::check_features`): a candidate that turned on
/// a baseline's or serde's optional behaviour (serde_json's
/// `float_roundtrip`, say) would change what the baseline is.
pub const SHARED_FEATURES: &[(&str, ())] = &[("serde_json", ()), ("prost", ()), ("bytes", ())];

/// Backends with their own derive: measured only where `payloads` carries
/// the workload ([`payloads::carries_native`]), owned only, and with no
/// conformance suite of their own (prost is a baseline; minicbor's derive is
/// a reference point that shares minicbor-serde's decoder).
pub const NATIVE: &[&str] = &["prost", "minicbor"];

/// Look a set up by name.
pub fn set(name: &str) -> anyhow::Result<&'static Set> {
    SETS.iter()
        .find(|s| s.name == name)
        .ok_or_else(|| anyhow::anyhow!("no set named {name}"))
}

/// Label a backend as reported.
#[must_use]
pub fn label(_set: &Set, backend: &str) -> String {
    backend.into()
}

/// One `-C target-cpu` choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Variant {
    /// Stable identifier.
    pub name: &'static str,
    /// `RUSTFLAGS`.
    pub rustflags: &'static str,
    /// Whether the counted (callgrind) tasks run this variant on this host.
    pub valgrind: bool,
}

/// The variants: x86-64 baseline (which decides: #989 section 7 asks for the
/// frontier "at a portable build"), and this host's CPU (for information:
/// what an application built for its machine gets). No CBOR candidate picks
/// a SIMD path, so there is no intermediate level to tell apart.
#[must_use]
pub fn variants() -> Vec<Variant> {
    vec![
        Variant {
            name: "portable",
            rustflags: "-C target-cpu=x86-64",
            valgrind: true,
        },
        Variant {
            name: "native",
            rustflags: "-C target-cpu=native",
            valgrind: false,
        },
    ]
}

/// Look a variant up by name.
pub fn variant(name: &str) -> anyhow::Result<Variant> {
    variants()
        .into_iter()
        .find(|v| v.name == name)
        .ok_or_else(|| anyhow::anyhow!("no variant {name}"))
}

/// An operation cell's shape, as `cell` takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    /// `decode`, `encode`, `encode-into`.
    pub op: &'static str,
    /// `owned`, `borrowed`.
    pub form: &'static str,
    /// `shared`, `unique`.
    pub arrival: &'static str,
}

/// Every operation shape. Encodes have no form or arrival of their own.
pub const SHAPES: &[Shape] = &[
    Shape {
        op: "decode",
        form: "owned",
        arrival: "shared",
    },
    Shape {
        op: "decode",
        form: "owned",
        arrival: "unique",
    },
    Shape {
        op: "decode",
        form: "borrowed",
        arrival: "shared",
    },
    Shape {
        op: "decode",
        form: "borrowed",
        arrival: "unique",
    },
    Shape {
        op: "encode",
        form: "owned",
        arrival: "shared",
    },
    Shape {
        op: "encode-into",
        form: "owned",
        arrival: "shared",
    },
];

/// The shapes the timed and end-to-end protocols run: the decode a server
/// does (owned, one shared frame) and the encode it does (`to_vec`).
pub const TIMED_SHAPES: &[Shape] = &[SHAPES[0], SHAPES[4]];

/// The workloads the decision rule is evaluated on.
pub const KYNOS: &[&str] = &["json-small", "echo-post", "json-large"];

/// Workloads only decoded: `skip-unknown` (what it measures is skipping; its
/// encode would be of a smaller document than its bytes), and those whose
/// bytes must be refused (nothing to encode).
pub const DECODE_ONLY: &[&str] = &["skip-unknown", "reject-truncated", "reject-shape"];

/// `shapes` as they apply to `workload` for `backend`: the decode shapes
/// alone (they come first in every shape list) for a [`DECODE_ONLY`]
/// workload; for a [`NATIVE`] backend, only the owned shapes, and nothing
/// for a workload it does not carry.
#[must_use]
pub fn for_workload(backend: &str, workload: &str, shapes: &'static [Shape]) -> Vec<Shape> {
    let native = NATIVE.contains(&backend);
    if native && !payloads::carries_native(workload) {
        return Vec::new();
    }
    let decode_only = DECODE_ONLY.contains(&workload);
    shapes
        .iter()
        .copied()
        .filter(|s| !decode_only || s.op == "decode")
        .filter(|s| !native || s.form == "owned")
        .collect()
}

/// The variants callgrind counts on the measured host; the report expects
/// exactly these.
pub const COUNTED_VARIANTS: &[&str] = &["portable"];

/// The variant whose hardware cycles decide rule 1.
pub const DECIDING_VARIANT: &str = "portable";

/// The shapes hardware cycles are taken at for `workload`: the server's own
/// decode and encode everywhere, and every shape at the Kynos shapes, whose
/// arrival and reused-buffer tables read them. Nothing measured goes
/// unreported.
#[must_use]
pub fn hw_shapes(backend: &str, workload: &str) -> Vec<Shape> {
    for_workload(
        backend,
        workload,
        if KYNOS.contains(&workload) {
            SHAPES
        } else {
            TIMED_SHAPES
        },
    )
}

/// Whether `backend` is a CBOR candidate the conformance suite runs.
#[must_use]
pub fn conformance_subject(backend: &str) -> bool {
    !NATIVE.contains(&backend) && backend != "serde_json"
}

/// The entries of `set` served end to end: the baselines through Kynos's own
/// bodies and every serde CBOR candidate through the worked codec, never a
/// native reference point.
#[must_use]
pub fn e2e_entries(set: &Set) -> Vec<&'static str> {
    set.backends
        .iter()
        .copied()
        .filter(|b| *b == "prost" || !NATIVE.contains(b))
        .collect()
}

/// The floors served end to end, one per format an entry speaks.
pub const FLOORS: &[&str] = &["floor-json", "floor-cbor", "floor-protobuf"];

/// The floor an end-to-end entry is weighed against.
#[must_use]
pub fn floor_of(entry: &str) -> &'static str {
    match entry {
        "serde_json" => "floor-json",
        "prost" => "floor-protobuf",
        _ => "floor-cbor",
    }
}
