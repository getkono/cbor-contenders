//! The decision rule of kynos #989 section 7, entry by entry.
//!
//! CBOR is admitted in-tree only if all five clauses hold:
//!
//! 1. A written use case an outside `BodyCodec` cannot serve. Settled by
//!    kynos #998 before anything here was measured: none exists. The report
//!    states it; nothing here computes it.
//! 2. On at least one of axes 1 to 4 (decode CPU, encode CPU, heap, encoded
//!    size) a candidate is on the frontier against serde_json and prost at
//!    the portable build: more than the margin better than both, on a Kynos
//!    workload, at the server's own shape (owned decode of a shared frame,
//!    `to_vec` encode). The margin is ε = 5 %, or on a cycles axis the A/A
//!    band at that cell when wider; a cycles advantage counts only where
//!    callgrind's estimated cycles at the same build point the same way.
//!    (aarch64, which #989 also names, is not measured; see the README.)
//! 3. No silent conformance difference: the suite's gate at every variant,
//!    no wrong answer in any measured cell or end-to-end route, no fuzz
//!    divergence, no undefined behaviour under Miri, and the worked codec's
//!    section 5 profile held on every case.
//! 4. No open soundness issue, no advisory affecting the pinned version, and
//!    a maintained crate (a release or substantive commit within twelve
//!    months of `results/maintenance.toml`'s `as_of` date).
//! 5. A licence Kynos's policy accepts (`references/kynos-deny-allow.txt`).
//!
//! Each clause is `pass`, `fail` or `pending` (its evidence is not all
//! there), with the reason.

use serde_json::{Value, json};

use super::data::{Data, Key};
use crate::matrix::{DECIDING_VARIANT, KYNOS, SETS};

/// The ε of ε-dominance.
pub const EPSILON: f64 = 0.05;

/// One clause's outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum Clause {
    Pass(String),
    Fail(String),
    Pending(String),
}

impl Clause {
    pub fn word(&self) -> &'static str {
        match self {
            Self::Pass(_) => "pass",
            Self::Fail(_) => "fail",
            Self::Pending(_) => "pending",
        }
    }

    pub fn why(&self) -> &str {
        match self {
            Self::Pass(s) | Self::Fail(s) | Self::Pending(s) => s,
        }
    }
}

/// The verdict on one candidate.
#[derive(Debug, Clone)]
pub struct Verdict {
    pub entry: &'static str,
    pub krate: &'static str,
    pub clauses: [Clause; 5],
}

impl Verdict {
    /// Admitted in-tree: every clause passes.
    pub fn admitted(&self) -> bool {
        self.clauses.iter().all(|c| matches!(c, Clause::Pass(_)))
    }
}

/// The candidates, each with its set and crate.
pub fn candidates() -> Vec<(&'static str, &'static str, &'static str)> {
    SETS.iter()
        .filter(|s| s.name != "baseline")
        .flat_map(|s| s.backends.iter().map(move |b| (*b, s.costs[0].0, s.name)))
        .collect()
}

/// One axis of clause 2: what it reads. Lower is better on every one.
#[derive(Debug, Clone, Copy)]
pub struct Axis {
    pub name: &'static str,
    pub op: &'static str,
    /// `cycles` (hw.json); `allocations` or `bytes` (alloc.json); `raw`,
    /// `gzip` or `zstd` (wire.json).
    pub field: &'static str,
}

/// Axes 1 to 4 (#989 section 2), each figure its own axis: decode and
/// encode cycles; heap blocks and bytes per decode and per encode; encoded
/// size raw, gzip and zstd.
pub const AXES: &[Axis] = &[
    Axis {
        name: "decode cycles",
        op: "decode",
        field: "cycles",
    },
    Axis {
        name: "encode cycles",
        op: "encode",
        field: "cycles",
    },
    Axis {
        name: "decode heap blocks",
        op: "decode",
        field: "allocations",
    },
    Axis {
        name: "decode heap bytes",
        op: "decode",
        field: "bytes",
    },
    Axis {
        name: "encode heap blocks",
        op: "encode",
        field: "allocations",
    },
    Axis {
        name: "encode heap bytes",
        op: "encode",
        field: "bytes",
    },
    Axis {
        name: "encoded size",
        op: "encode",
        field: "raw",
    },
    Axis {
        name: "encoded size gzip",
        op: "encode",
        field: "gzip",
    },
    Axis {
        name: "encoded size zstd",
        op: "encode",
        field: "zstd",
    },
];

/// An entry's figure on `axis` at `workload` (portable, the server's shape).
pub fn figure(data: &Data, axis: &Axis, entry: &str, workload: &str) -> Option<f64> {
    let key = Key::new(DECIDING_VARIANT, entry, workload, axis.op);
    match axis.field {
        "cycles" => data.host.hw.get(&key, "cycles"),
        "allocations" | "bytes" => data.host.alloc.get(&key, axis.field),
        field => data
            .wire
            .iter()
            .find(|r| r["backend"] == entry && r["workload"] == workload)
            .and_then(|r| r[field].as_f64()),
    }
}

/// callgrind's estimated cycles for the same cell, which a cycles
/// advantage must agree with.
pub fn counted(data: &Data, axis: &Axis, entry: &str, workload: &str) -> Option<f64> {
    data.callgrind
        .get(&Key::new(DECIDING_VARIANT, entry, workload, axis.op), "est_cycles")
}

/// The A/A band at a cycles cell: the relative gap between a baseline and
/// its copy, in the medians or in any one round.
pub fn aa_band(data: &Data, axis: &Axis, baseline: &str, workload: &str) -> Option<f64> {
    if axis.field != "cycles" {
        return Some(0.0);
    }
    let row = |b: &str| {
        data.host
            .hw
            .row(&Key::new(DECIDING_VARIANT, b, workload, axis.op))
            .cloned()
    };
    let (a, copy) = (row(baseline)?, row(&format!("{baseline}#aa"))?);
    let gap = |x: f64, y: f64| (x - y).abs() / y.max(f64::MIN_POSITIVE);
    let mut band = gap(a["cycles"].as_f64()?, copy["cycles"].as_f64()?);
    let rounds = |r: &Value| -> Vec<f64> {
        r["rounds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x["cycles"].as_f64())
            .collect()
    };
    for (x, y) in rounds(&a).into_iter().zip(rounds(&copy)) {
        band = band.max(gap(x, y));
    }
    Some(band)
}

/// Whether `entry` beats both baselines on `axis` at `workload` by more
/// than the margin (ε, or with `band` the A/A band where wider), a cycles
/// win corroborated by callgrind; `Err` names a figure that is missing.
fn beats_both(data: &Data, axis: &Axis, entry: &str, workload: &str, band: bool) -> Result<bool, String> {
    let mine = figure(data, axis, entry, workload).ok_or_else(|| format!("{} at {workload}", axis.name))?;
    let mut beats = true;
    for baseline in ["serde_json", "prost"] {
        let (Some(theirs), Some(aa)) = (
            figure(data, axis, baseline, workload),
            aa_band(data, axis, baseline, workload),
        ) else {
            return Err(format!("{} at {workload} ({baseline})", axis.name));
        };
        let margin = if band { EPSILON.max(aa) } else { EPSILON };
        let better = mine < theirs * (1.0 - margin);
        let agrees = axis.field != "cycles"
            || matches!(
                (counted(data, axis, entry, workload), counted(data, axis, baseline, workload)),
                (Some(m), Some(t)) if m < t
            );
        beats &= better && agrees;
    }
    Ok(beats)
}

/// The trust class of the hardware cycles a win on `axis` at `workload`
/// rests on: `canonical` only if the entry's, both baselines' and their A/A
/// copies' rows all are. Every other axis is deterministic and settled.
pub fn win_trust(data: &Data, axis: &Axis, entry: &str, workload: &str) -> &'static str {
    if axis.field != "cycles" {
        return "canonical";
    }
    let canonical = [entry, "serde_json", "serde_json#aa", "prost", "prost#aa"]
        .iter()
        .all(|b| {
            data.host
                .hw
                .row(&Key::new(DECIDING_VARIANT, b, workload, axis.op))
                .is_some_and(|r| r["trust"] == "canonical")
        });
    if canonical { "canonical" } else { "solo" }
}

/// #989 section 6's cross-check on one route: an entry's timed throughput
/// against serde_json's, beside what the counted codec share allows.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossCheck {
    pub route: &'static str,
    pub entry: &'static str,
    /// Timed throughput over serde_json's (`1.10` is 10 % more requests per
    /// second).
    pub timed: f64,
    /// Counted speedup over serde_json: serde_json's estimated cycles per
    /// request over the entry's.
    pub counted: f64,
    /// serde_json's codec share against the entry's floor, `s = (serde_json
    /// − floor) ÷ serde_json`.
    pub share: f64,
    /// The most any codec could speed the request up: `1 ÷ (1 − s)`, the
    /// speedup if the entry's codec cost nothing.
    pub bound: f64,
    /// serde_json's A/A throughput gap on the route, in the medians or any
    /// rep; 0 without a copy.
    pub band: f64,
    /// The timed gain exceeds the bound beyond the band: a measurement error
    /// to explain, not a result.
    pub flagged: bool,
}

/// [`CrossCheck`] for every CBOR entry served end to end and every route
/// where both the timed and the counted figures exist.
#[must_use]
pub fn cross_checks(data: &Data) -> Vec<CrossCheck> {
    let counted = |entry: &str, route: &str| {
        data.e2e_count
            .iter()
            .find(|r| r["backend"] == entry && r["route"] == route && r["variant"] == DECIDING_VARIANT)
            .and_then(|r| r["est_cycles"].as_f64())
            .filter(|c| *c > 0.0)
    };
    let timed = |entry: &str, route: &str| {
        data.host
            .e2e_time
            .iter()
            .find(|r| r["backend"] == entry && r["route"] == route && r["variant"] == DECIDING_VARIANT)
    };
    let mut out = Vec::new();
    for route in crate::endtoend::ROUTES {
        let route = route.0;
        let Some(sj) = timed("serde_json", route) else {
            continue;
        };
        let Some(sj_rps) = sj["rps"].as_f64().filter(|r| *r > 0.0) else {
            continue;
        };
        let rps_of = |r: &Value| r["rps"].as_f64();
        let band = timed("serde_json#aa", route).map_or(0.0, |aa| {
            let reps = |r: &Value| -> Vec<f64> {
                r["reps"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|x| x["closed"]["rps"].as_f64())
                    .collect()
            };
            let gap = |x: f64, y: f64| if x > 0.0 { (y / x - 1.0).abs() } else { 0.0 };
            let median = rps_of(aa).map_or(0.0, |a| gap(sj_rps, a));
            reps(sj)
                .into_iter()
                .zip(reps(aa))
                .map(|(x, y)| gap(x, y))
                .fold(median, f64::max)
        });
        for entry in crate::matrix::e2e_entries_all() {
            if crate::endtoend::format_of(entry) != codecs::Format::Cbor {
                continue;
            }
            let (Some(me), Some(sj_count), Some(my_count), Some(floor)) = (
                timed(entry, route),
                counted("serde_json", route),
                counted(entry, route),
                counted(crate::matrix::floor_of(entry), route),
            ) else {
                continue;
            };
            let Some(my_rps) = rps_of(me) else { continue };
            let share = ((sj_count - floor) / sj_count).clamp(0.0, 1.0);
            let bound = if share < 1.0 {
                1.0 / (1.0 - share)
            } else {
                f64::INFINITY
            };
            let ratio = my_rps / sj_rps;
            out.push(CrossCheck {
                route,
                entry,
                timed: ratio,
                counted: sj_count / my_count,
                share,
                bound,
                band,
                flagged: ratio > bound * (1.0 + band),
            });
        }
    }
    out
}

/// Clause 2 for `entry`. A win on a deterministic axis (heap, size) is
/// settled; a cycles win measured at `solo` trust is provisional, so it
/// leaves the clause pending until a quiet host has measured it, as does a
/// cycles win beyond ε that only a `solo` A/A band hides.
fn frontier(data: &Data, entry: &str) -> Clause {
    // A timed gain the counts cannot account for says the host's timing is
    // not to be read, and clause 2's cycles come from the same host: the
    // clause waits for the error to be explained, whatever it would say.
    let flagged: Vec<String> = cross_checks(data)
        .into_iter()
        .filter(|c| c.entry == entry && c.flagged)
        .map(|c| {
            format!(
                "{} ({:.2}× timed, at most {:.2}× by the counts)",
                c.route, c.timed, c.bound
            )
        })
        .collect();
    if !flagged.is_empty() {
        return Clause::Pending(format!(
            "measurement error to explain (#989 section 6): {}",
            flagged.join(", ")
        ));
    }
    let mut pending = Vec::new();
    let mut settled = Vec::new();
    let mut provisional = Vec::new();
    let mut hidden = Vec::new();
    for workload in KYNOS {
        for axis in AXES {
            let solo = win_trust(data, axis, entry, workload) != "canonical";
            match beats_both(data, axis, entry, workload, true) {
                Err(gap) => pending.push(gap),
                Ok(true) if solo => provisional.push(format!("{} at {workload}", axis.name)),
                Ok(true) => settled.push(format!("{} at {workload}", axis.name)),
                Ok(false) if solo && beats_both(data, axis, entry, workload, false) == Ok(true) => {
                    hidden.push(format!("{} at {workload}", axis.name));
                }
                Ok(false) => {}
            }
        }
    }
    if !settled.is_empty() {
        Clause::Pass(format!(
            "better than serde_json and prost beyond the margin on {}",
            settled.join(", ")
        ))
    } else if !provisional.is_empty() {
        Clause::Pending(format!(
            "provisional: better beyond the margin on {}, measured at solo trust; awaits a canonical rerun",
            provisional.join(", ")
        ))
    } else if !pending.is_empty() {
        Clause::Pending(format!("not measured: {}", pending.join(", ")))
    } else if !hidden.is_empty() {
        Clause::Pending(format!(
            "better beyond ε but inside the solo A/A band on {}; awaits a canonical rerun",
            hidden.join(", ")
        ))
    } else {
        Clause::Fail(
            "on no axis 1–4 at a Kynos workload better than both serde_json and prost beyond the margin".into(),
        )
    }
}

/// The depth `Json<T>` refuses past: serde_json's recursion limit, which the
/// worked codec matches.
pub const JSON_DEPTH: u32 = 128;

/// What a cell's `rejected` verdict means for clause 3. A `rejected` cell
/// is a valid workload the library refused (the bodies that must be refused
/// are judged `exact` or `wrong` instead):
///
/// - an operation the library does not have (`Unsupported`: ciborium has no
///   borrowing decode), or a workload a native format does not carry, is
///   recorded, not a wrong answer;
/// - a refusal with a syntax status (400) of a `nested-N` workload deeper
///   than [`JSON_DEPTH`] is the library's depth limit, which RFC 8949 leaves
///   to the implementation and `Json<T>` itself refuses: recorded;
/// - any other refusal is a wrong answer to a client, as a server answers
///   it: a body the oracle decodes and the library will not, or a value it
///   will not encode.
#[must_use]
pub fn refusal_is_wrong(workload: &str, detail: &str) -> bool {
    if detail.starts_with("Unsupported") || detail == "not carried by the native formats" {
        return false;
    }
    let deep = workload
        .strip_prefix("nested-")
        .and_then(|d| d.parse::<u32>().ok())
        .is_some_and(|d| d > JSON_DEPTH);
    // `Failure`'s display leads with its class; only `Data` answers 422.
    let syntax = !detail.starts_with("Data");
    !(deep && syntax)
}

/// The per-operation tables clause 3 reads, by name.
fn measured_tables(data: &Data) -> [(&'static str, &super::data::Table); 4] {
    [
        ("callgrind", &data.callgrind),
        ("alloc", &data.host.alloc),
        ("hw", &data.host.hw),
        ("time", &data.host.time),
    ]
}

/// Clause 3 for `entry` of `set`. It passes only on evidence: every source
/// it reads must hold rows for the entry, so a stage that did not run, or
/// that `results/skipped.toml` excused, leaves it pending, never passed.
fn conformance(data: &Data, entry: &str, set: &str) -> Clause {
    let mut failures = Vec::new();
    let mut pending = Vec::new();
    if crate::matrix::conformance_subject(entry) {
        for variant in crate::matrix::variants() {
            match data.host.conformance.get(&(variant.name.to_owned(), set.to_owned())) {
                None => pending.push(format!("conformance at {}", variant.name)),
                Some(report) => match report["backends"][entry]["gate"].as_str() {
                    Some("pass") => {}
                    Some("fail") => failures.push(format!(
                        "{} gating conformance failures at {}",
                        report["backends"][entry]["gating_failures"]
                            .as_array()
                            .map_or(0, Vec::len),
                        variant.name
                    )),
                    _ => pending.push(format!("conformance at {} did not finish", variant.name)),
                },
            }
        }
    } else {
        // minicbor's derive: no serde suite (its wire form has no faithful
        // schema, which is itself a failure of section 5).
        failures.push("its wire form (a struct as an array) has no faithful JSON Schema (section 5)".into());
    }
    let mut wrong = 0;
    let mut refused: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (name, table) in measured_tables(data) {
        let rows: Vec<(&super::data::Key, &Value)> = table.rows.iter().filter(|(k, _)| k.backend == entry).collect();
        if rows.is_empty() {
            pending.push(format!("no {name} cells"));
        }
        for (key, row) in rows {
            match row["verdict"].as_str() {
                Some("wrong") => wrong += 1,
                Some("rejected") if refusal_is_wrong(&key.workload, row["detail"].as_str().unwrap_or("")) => {
                    refused.insert(format!("{} {}", key.workload, key.op));
                }
                _ => {}
            }
        }
    }
    if wrong > 0 {
        failures.push(format!("{wrong} measured cells answered wrongly"));
    }
    if !refused.is_empty() {
        failures.push(format!(
            "refused valid workloads: {}",
            refused.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if crate::matrix::e2e_entries_all().contains(&entry) {
        for (name, rows) in [("e2e-count", &data.e2e_count), ("e2e-time", &data.host.e2e_time)] {
            let mine: Vec<&Value> = rows.iter().filter(|r| r["backend"] == entry).collect();
            if mine.is_empty() {
                pending.push(format!("no {name} routes"));
            }
            let e2e = mine.iter().filter(|r| r["verdict"] == "wrong").count();
            if e2e > 0 {
                failures.push(format!("{e2e} {name} routes answered wrongly"));
            }
        }
    }
    if crate::matrix::conformance_subject(entry) {
        let runs: Vec<&Value> = data.fuzz.iter().filter(|r| r["backend"] == entry).collect();
        for target in crate::fuzzing::FUZZ_TARGETS {
            if !runs.iter().any(|r| r["target"] == *target && r["finished"] == true) {
                pending.push(format!("fuzzing {target}"));
            }
        }
        // A sanitizer report is a divergence whatever the run recorded: a
        // run from before it was counted as one says so only there.
        let diverged = runs
            .iter()
            .filter(|r| r["divergence_found"] == true || r["sanitizer_report"].is_string())
            .count();
        if diverged > 0 {
            failures.push(format!("fuzzing diverged in {diverged} targets"));
        }
        match data
            .miri
            .iter()
            .find(|r| r["set"] == set)
            .and_then(|r| r["outcome"].as_str())
        {
            Some("pass") => {}
            Some("undefined-behavior") => failures.push("Miri reports undefined behaviour".into()),
            Some(other) => pending.push(format!("Miri: {other}")),
            None => pending.push("Miri".into()),
        }
    }
    if !failures.is_empty() {
        Clause::Fail(failures.join("; "))
    } else if !pending.is_empty() {
        Clause::Pending(format!("not measured: {}", pending.join(", ")))
    } else {
        Clause::Pass("no silent difference in any section, cell, route, fuzz target or Miri run".into())
    }
}

/// Clause 4 for `krate`.
fn soundness(data: &Data, krate: &str, set: &str) -> Clause {
    // Evidence that is missing makes the clause pending, but only once
    // everything present has been read: a failure found elsewhere stands.
    let mut failures = Vec::new();
    let mut pending = Vec::new();
    match data
        .soundness
        .get(krate)
        .and_then(|e| e.get("open"))
        .and_then(toml::Value::as_array)
    {
        Some(open) if !open.is_empty() => failures.push(format!("{} open soundness issues", open.len())),
        Some(_) => {}
        None => pending.push("no reviewed `open` list in soundness.toml"),
    }
    // A memory error AddressSanitizer caught while fuzzing is unsoundness in
    // the code the candidate adds (the fuzz targets and the oracle are safe
    // Rust), whatever else is recorded.
    let sanitized: Vec<String> = data
        .fuzz
        .iter()
        .filter(|r| r["set"] == set && r["sanitizer_report"].is_string())
        .map(|r| {
            format!(
                "AddressSanitizer report fuzzing {}",
                r["target"].as_str().unwrap_or("?")
            )
        })
        .collect();
    if !sanitized.is_empty() {
        failures.push(sanitized.join(", "));
    }
    match data.footprint.iter().find(|r| r["set"] == set && r["crate"] == krate) {
        None => pending.push("no footprint"),
        Some(row) => {
            let advisories: Vec<String> = row["crates"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|c| c["advisories"].as_array().into_iter().flatten().map(move |a| (c, a)))
                // Whatever its `informational` kind: RustSec files an
                // unsoundness as `informational = "unsound"`, and an
                // unmaintained crate as `"unmaintained"`; both fail.
                .filter(|(_, a)| a["affects_pinned_version"] == true)
                .map(|(c, a)| {
                    format!(
                        "{} {}",
                        c["name"].as_str().unwrap_or("?"),
                        a["id"].as_str().unwrap_or("?")
                    )
                })
                .collect();
            if !advisories.is_empty() {
                failures.push(format!("advisories: {}", advisories.join(", ")));
            }
        }
    }
    match maintained(data, krate) {
        Some(true) => {}
        Some(false) => failures.push("no release or code change within twelve months".into()),
        None => pending.push("no maintenance.toml entry"),
    }
    if !failures.is_empty() {
        Clause::Fail(failures.join("; "))
    } else if !pending.is_empty() {
        Clause::Pending(pending.join("; "))
    } else {
        Clause::Pass("no open soundness issue or advisory, and maintained".into())
    }
}

/// Days since the civil date `y-m-d` began, for date arithmetic without a
/// calendar crate.
fn days(date: &str) -> Option<i64> {
    let mut parts = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    Some(365 * y + y / 4 - y / 100 + y / 400 + (153 * m + 2) / 5 + d)
}

/// Whether `krate` had a release or a substantive commit within 365 days of
/// the file's `as_of`.
pub fn maintained(data: &Data, krate: &str) -> Option<bool> {
    let as_of = days(data.maintenance.get("as_of")?.as_str()?)?;
    let entry = data.maintenance.get(krate)?;
    let latest = ["last_release", "last_substantive_commit"]
        .iter()
        .filter_map(|k| entry.get(*k).and_then(toml::Value::as_str).and_then(days))
        .max()?;
    Some(as_of - latest <= 365)
}

/// Whether an SPDX expression is acceptable under `allowed`. `OR` (or the
/// legacy `/`) takes any side, `AND` every side, `AND` binds tighter than
/// `OR`, and parentheses group. `X WITH Y` is acceptable only if the allow
/// list names it whole (`Apache-2.0 WITH LLVM-exception`), as cargo-deny's
/// does. An expression that does not parse is not acceptable.
pub fn licence_ok(expression: &str, allowed: &[String]) -> bool {
    let spaced = expression.replace('(', " ( ").replace(')', " ) ").replace('/', " OR ");
    let tokens: Vec<&str> = spaced.split_whitespace().collect();
    let mut at = 0;
    let ok = spdx_or(&tokens, &mut at, allowed);
    ok.is_some_and(|ok| ok && at == tokens.len())
}

/// The operators and brackets of an SPDX expression: never a licence name.
const SPDX_SYNTAX: &[&str] = &["(", ")", "OR", "AND", "WITH"];

/// `and ("OR" and)*`.
fn spdx_or(tokens: &[&str], at: &mut usize, allowed: &[String]) -> Option<bool> {
    let mut ok = spdx_and(tokens, at, allowed)?;
    while tokens.get(*at) == Some(&"OR") {
        *at += 1;
        ok |= spdx_and(tokens, at, allowed)?;
    }
    Some(ok)
}

/// `term ("AND" term)*`.
fn spdx_and(tokens: &[&str], at: &mut usize, allowed: &[String]) -> Option<bool> {
    let mut ok = spdx_term(tokens, at, allowed)?;
    while tokens.get(*at) == Some(&"AND") {
        *at += 1;
        ok &= spdx_term(tokens, at, allowed)?;
    }
    Some(ok)
}

/// `"(" or ")"`, or `licence ["WITH" exception]`.
fn spdx_term(tokens: &[&str], at: &mut usize, allowed: &[String]) -> Option<bool> {
    let token = *tokens.get(*at)?;
    *at += 1;
    if token == "(" {
        let ok = spdx_or(tokens, at, allowed)?;
        (tokens.get(*at) == Some(&")")).then_some(())?;
        *at += 1;
        return Some(ok);
    }
    if SPDX_SYNTAX.contains(&token) {
        return None;
    }
    let name = if tokens.get(*at) == Some(&"WITH") {
        let exception = tokens.get(*at + 1).filter(|e| !SPDX_SYNTAX.contains(*e))?;
        *at += 2;
        format!("{token} WITH {exception}")
    } else {
        token.to_owned()
    };
    Some(allowed.contains(&name))
}

/// Clause 5 for `krate`: its own licence and that of every runtime crate it
/// adds.
fn licence(data: &Data, krate: &str, set: &str) -> Clause {
    let Some(row) = data.footprint.iter().find(|r| r["set"] == set && r["crate"] == krate) else {
        return Clause::Pending("no footprint".into());
    };
    let refused: Vec<String> = row["crates"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["build_only"] == false)
        .filter(|c| !c["licence"].as_str().is_some_and(|l| licence_ok(l, &data.licences)))
        .map(|c| {
            format!(
                "{} ({})",
                c["name"].as_str().unwrap_or("?"),
                c["licence"].as_str().unwrap_or("no licence")
            )
        })
        .collect();
    if refused.is_empty() {
        Clause::Pass("every crate it adds is under a licence Kynos's deny.toml allows".into())
    } else {
        Clause::Fail(format!("not on Kynos's allow list: {}", refused.join(", ")))
    }
}

/// Every candidate's verdict.
pub fn judge(data: &Data) -> Vec<Verdict> {
    candidates()
        .into_iter()
        .map(|(entry, krate, set)| Verdict {
            entry,
            krate,
            clauses: [
                Clause::Fail("kynos #998: no written use case an outside BodyCodec cannot serve".into()),
                frontier(data, entry),
                conformance(data, entry, set),
                soundness(data, krate, set),
                licence(data, krate, set),
            ],
        })
        .collect()
}

/// `report/verdict.json`.
pub fn to_json(verdicts: &[Verdict]) -> Value {
    json!({
        "rule": "kynos #989 section 7",
        "admitted": verdicts.iter().any(Verdict::admitted),
        "entries": verdicts.iter().map(|v| json!({
            "entry": v.entry,
            "crate": v.krate,
            "admitted": v.admitted(),
            "clauses": v.clauses.iter().enumerate().map(|(i, c)| json!({ "clause": i + 1, "outcome": c.word(), "why": c.why() })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One fuzz run of ciborium that a sanitizer stopped.
    fn sanitized(data: &mut Data) {
        data.fuzz.push(json!({
            "set": "ciborium", "backend": "ciborium", "config": "portable", "target": "decode_value",
            "finished": true, "divergence_found": false,
            "sanitizer_report": "==9==ERROR: AddressSanitizer: heap-buffer-overflow",
        }));
    }

    /// An open issue fails clause 4 although the footprint and the
    /// maintenance entry, which would only make it pending, are missing.
    #[test]
    fn a_failure_outranks_missing_evidence() {
        let mut data = Data {
            soundness: toml::from_str("[ciborium]\nopen = [\"#1 unsound\"]\n").unwrap(),
            ..Data::default()
        };
        let c4 = soundness(&data, "ciborium", "ciborium");
        assert!(matches!(&c4, Clause::Fail(why) if why.contains("1 open")), "{c4:?}");
        data.soundness = toml::from_str("[ciborium]\nopen = []\n").unwrap();
        assert_eq!(soundness(&data, "ciborium", "ciborium").word(), "pending");
    }

    /// RustSec files unsoundness as informational; it still fails.
    #[test]
    fn an_informational_advisory_on_the_pinned_version_fails() {
        let mut data = Data {
            soundness: toml::from_str("[ciborium]\nopen = []\n").unwrap(),
            maintenance: toml::from_str("as_of = \"2026-11-11\"\n[ciborium]\nlast_release = \"2026-10-01\"\n").unwrap(),
            ..Data::default()
        };
        let advisory = |affects: bool| {
            json!({ "set": "ciborium", "crate": "ciborium", "crates": [{ "name": "ciborium", "advisories": [
                { "id": "RUSTSEC-2099-0001", "informational": "unsound", "affects_pinned_version": affects },
            ] }] })
        };
        data.footprint = vec![advisory(false)];
        assert_eq!(soundness(&data, "ciborium", "ciborium").word(), "pass");
        data.footprint = vec![advisory(true)];
        let c4 = soundness(&data, "ciborium", "ciborium");
        assert!(
            matches!(&c4, Clause::Fail(why) if why.contains("RUSTSEC-2099-0001")),
            "{c4:?}"
        );
    }

    /// Everything clause 3 reads, clean, for ciborium.
    fn clean_ciborium() -> Data {
        use super::super::data::Table;
        let mut data = Data::default();
        for variant in crate::matrix::variants() {
            data.host.conformance.insert(
                (variant.name.to_owned(), "ciborium".to_owned()),
                json!({ "backends": { "ciborium": { "gate": "pass" } } }),
            );
        }
        let table = || {
            let mut t = Table::default();
            t.rows.insert(
                Key::new(DECIDING_VARIANT, "ciborium", "json-small", "decode"),
                json!({ "verdict": "exact" }),
            );
            t
        };
        data.callgrind = table();
        data.host.alloc = table();
        data.host.hw = table();
        data.host.time = table();
        for target in crate::fuzzing::FUZZ_TARGETS {
            data.fuzz.push(json!({ "set": "ciborium", "backend": "ciborium", "target": target, "finished": true, "divergence_found": false }));
        }
        data.miri.push(json!({ "set": "ciborium", "outcome": "pass" }));
        let route = json!({ "backend": "ciborium", "route": "echo-post" });
        data.e2e_count.push(route.clone());
        data.host.e2e_time.push(route);
        data
    }

    #[test]
    fn clause_3_passes_only_on_every_source() {
        assert_eq!(conformance(&clean_ciborium(), "ciborium", "ciborium").word(), "pass");
        // A stage that did not run, or that skipped.toml excused, leaves an
        // empty table: pending, never passed.
        let without: [fn(&mut Data); 6] = [
            |d| d.host.time.rows.clear(),
            |d| d.callgrind.rows.clear(),
            |d| d.host.e2e_time.clear(),
            |d| d.e2e_count.clear(),
            |d| d.fuzz.truncate(1),
            |d| d.miri.clear(),
        ];
        for (i, drop) in without.iter().enumerate() {
            let mut data = clean_ciborium();
            drop(&mut data);
            assert_eq!(conformance(&data, "ciborium", "ciborium").word(), "pending", "case {i}");
        }
    }

    #[test]
    fn a_wrong_time_cell_or_route_fails_clause_3() {
        let mut data = clean_ciborium();
        for row in data.host.time.rows.values_mut() {
            row["verdict"] = json!("wrong");
        }
        assert_eq!(conformance(&data, "ciborium", "ciborium").word(), "fail");
        let mut data = clean_ciborium();
        data.host.e2e_time[0]["verdict"] = json!("wrong");
        assert_eq!(conformance(&data, "ciborium", "ciborium").word(), "fail");
    }

    /// A refused valid workload is a wrong answer, but a 400 past the depth
    /// `Json<T>` refuses is a recorded limit, as is an operation the library
    /// does not have.
    #[test]
    fn a_refused_valid_workload_fails_unless_it_is_a_depth_limit() {
        let refusing = |workload: &str, detail: &str| {
            let mut data = clean_ciborium();
            data.callgrind.rows.insert(
                Key::new(DECIDING_VARIANT, "ciborium", workload, "decode"),
                json!({ "verdict": "rejected", "detail": detail }),
            );
            conformance(&data, "ciborium", "ciborium").word()
        };
        assert_eq!(refusing("nested-257", "Syntax: recursion limit exceeded"), "pass");
        assert_eq!(refusing("nested-10000", "Syntax: recursion limit exceeded"), "pass");
        assert_eq!(refusing("nested-255", "Syntax: recursion limit exceeded"), "pass");
        assert_eq!(refusing("nested-100", "Syntax: recursion limit exceeded"), "fail");
        assert_eq!(refusing("nested-10000", "Data: invalid type"), "fail");
        assert_eq!(refusing("floats", "Syntax: invalid"), "fail");
        assert_eq!(refusing("json-large", "Unsupported: does not borrow"), "pass");
    }

    /// Recorded as no divergence (as runs before the fix were), the report
    /// still fails clause 3 on it, and clause 4 as unsoundness.
    #[test]
    fn a_sanitizer_report_fails_clauses_3_and_4() {
        let mut data = Data::default();
        sanitized(&mut data);
        let c3 = conformance(&data, "ciborium", "ciborium");
        assert!(
            matches!(&c3, Clause::Fail(why) if why.contains("fuzzing diverged")),
            "{c3:?}"
        );
        let c4 = soundness(&data, "ciborium", "ciborium");
        assert!(
            matches!(&c4, Clause::Fail(why) if why.contains("AddressSanitizer")),
            "{c4:?}"
        );
    }

    #[test]
    fn licence_expressions() {
        let allowed = vec!["MIT".to_owned(), "Apache-2.0".to_owned()];
        assert!(licence_ok("MIT OR Apache-2.0", &allowed));
        assert!(licence_ok("MIT/Apache-2.0", &allowed));
        assert!(licence_ok("Apache-2.0", &allowed));
        assert!(!licence_ok("BlueOak-1.0.0", &allowed));
        assert!(!licence_ok("MIT AND BlueOak-1.0.0", &allowed));
    }

    #[test]
    fn licence_expressions_group_and_bind_as_spdx_says() {
        let allowed: Vec<String> = ["MIT", "Apache-2.0", "Unicode-3.0", "Apache-2.0 WITH LLVM-exception"]
            .map(str::to_owned)
            .to_vec();
        assert!(!licence_ok("(MIT OR Apache-2.0) AND BlueOak-1.0.0", &allowed));
        assert!(licence_ok("(MIT OR Apache-2.0) AND Unicode-3.0", &allowed));
        // AND binds tighter than OR.
        assert!(licence_ok("BlueOak-1.0.0 AND MIT OR Apache-2.0", &allowed));
        assert!(!licence_ok("BlueOak-1.0.0 AND (MIT OR Apache-2.0)", &allowed));
        assert!(licence_ok("Apache-2.0 WITH LLVM-exception OR MIT", &allowed));
        assert!(!licence_ok("MIT WITH LLVM-exception", &allowed));
        assert!(licence_ok("((MIT))", &allowed));
        for broken in ["", "MIT OR", "(MIT", "MIT)", "AND MIT", "MIT WITH", "MIT Apache-2.0"] {
            assert!(!licence_ok(broken, &allowed), "{broken:?}");
        }
    }

    #[test]
    fn twelve_months_is_counted_in_days() {
        assert_eq!(days("2026-11-10").unwrap() - days("2025-11-10").unwrap(), 365);
        assert_eq!(days("2024-03-01").unwrap() - days("2024-02-28").unwrap(), 2);
    }

    #[test]
    fn nothing_measured_is_pending_not_passed() {
        let data = Data::default();
        let verdicts = judge(&data);
        assert!(!verdicts.is_empty());
        for v in &verdicts {
            assert!(!v.admitted());
            assert_eq!(v.clauses[0].word(), "fail");
            assert_eq!(v.clauses[1].word(), "pending", "{v:?}");
        }
    }

    #[test]
    fn a_cycles_win_inside_the_band_or_against_the_count_is_no_win() {
        use super::super::data::Table;
        let mut data = Data::default();
        let mut hw = Table::default();
        let mut cg = Table::default();
        let mut alloc = Table::default();
        for (entry, cycles, est, bytes) in [
            ("serde_json", 100.0, 100.0, 50.0),
            ("serde_json#aa", 100.0, 0.0, 0.0),
            ("prost", 100.0, 100.0, 50.0),
            ("prost#aa", 100.0, 0.0, 0.0),
            ("ciborium", 80.0, 120.0, 50.0),
        ] {
            for workload in KYNOS {
                for op in ["decode", "encode"] {
                    let key = Key::new(DECIDING_VARIANT, entry, workload, op);
                    hw.rows.insert(key.clone(), json!({ "cycles": cycles, "rounds": [] }));
                    cg.rows.insert(key.clone(), json!({ "est_cycles": est }));
                    alloc.rows.insert(key, json!({ "allocations": bytes, "bytes": bytes }));
                }
            }
        }
        data.host.hw = hw;
        data.callgrind = cg;
        data.host.alloc = alloc;
        for entry in ["serde_json", "prost", "ciborium"] {
            for workload in KYNOS {
                data.wire
                    .push(json!({ "backend": entry, "workload": workload, "raw": 100, "gzip": 100, "zstd": 100 }));
            }
        }
        // Faster in cycles, but callgrind says slower: no win; nothing else
        // better beyond ε.
        assert_eq!(frontier(&data, "ciborium").word(), "fail");
        // Smaller on the wire by 10 %: a win.
        for row in data.wire.iter_mut().filter(|r| r["backend"] == "ciborium") {
            row["raw"] = json!(90);
        }
        assert_eq!(frontier(&data, "ciborium").word(), "pass");
    }

    /// Hardware cycles for every entry the frontier reads at the Kynos
    /// workloads, taken at `trust`, with callgrind agreeing.
    fn cycles(trust: &str, ciborium: f64) -> Data {
        use super::super::data::Table;
        let mut data = Data::default();
        let (mut hw, mut cg) = (Table::default(), Table::default());
        for (entry, cycles) in [
            ("serde_json", 100.0),
            ("serde_json#aa", 100.0),
            ("prost", 100.0),
            ("prost#aa", 100.0),
            ("ciborium", ciborium),
        ] {
            for workload in KYNOS {
                for op in ["decode", "encode"] {
                    let key = Key::new(DECIDING_VARIANT, entry, workload, op);
                    hw.rows
                        .insert(key.clone(), json!({ "cycles": cycles, "rounds": [], "trust": trust }));
                    cg.rows.insert(key, json!({ "est_cycles": cycles }));
                }
            }
        }
        data.host.hw = hw;
        data.callgrind = cg;
        data
    }

    /// A cycles win at `solo` trust is provisional: pending, not passed.
    #[test]
    fn a_solo_cycles_win_is_provisional() {
        let solo = frontier(&cycles("solo", 80.0), "ciborium");
        assert!(
            matches!(&solo, Clause::Pending(why) if why.starts_with("provisional")),
            "{solo:?}"
        );
        let canonical = frontier(&cycles("canonical", 80.0), "ciborium");
        assert!(
            matches!(&canonical, Clause::Pass(why) if why.contains("decode cycles")),
            "{canonical:?}"
        );
    }

    /// Counted: serde_json 100 cycles a request over a floor of 80, so a
    /// free codec would be at most 1.25× faster. Timed `ratio`× faster.
    fn routed(ratio: f64) -> Data {
        let mut data = Data::default();
        for (entry, est) in [("serde_json", 100.0), ("ciborium", 90.0), ("floor-cbor", 80.0)] {
            data.e2e_count.push(
                json!({ "backend": entry, "route": "echo-post", "variant": DECIDING_VARIANT, "est_cycles": est }),
            );
        }
        for (entry, rps) in [
            ("serde_json", 1000.0),
            ("serde_json#aa", 1010.0),
            ("ciborium", 1000.0 * ratio),
        ] {
            data.host
                .e2e_time
                .push(json!({ "backend": entry, "route": "echo-post", "variant": DECIDING_VARIANT, "rps": rps }));
        }
        data
    }

    #[test]
    fn a_timed_gain_beyond_the_counted_share_is_flagged() {
        let ok = cross_checks(&routed(1.20));
        assert_eq!(ok.len(), 1);
        assert!(
            (ok[0].bound - 1.25).abs() < 1e-9 && (ok[0].share - 0.2).abs() < 1e-9,
            "{ok:?}"
        );
        assert!(!ok[0].flagged, "{ok:?}");
        // 1.26 is beyond 1.25 but inside the 1 % A/A band; 1.40 is not.
        assert!(!cross_checks(&routed(1.26))[0].flagged);
        let off = cross_checks(&routed(1.40));
        assert!(off[0].flagged, "{off:?}");
        let c2 = frontier(&routed(1.40), "ciborium");
        assert!(
            matches!(&c2, Clause::Pending(why) if why.contains("section 6")),
            "{c2:?}"
        );
    }

    /// Fewer heap blocks, or a smaller compressed body, is a win of its own.
    #[test]
    fn blocks_and_compressed_sizes_are_axes_of_their_own() {
        use super::super::data::Table;
        let mut data = Data::default();
        let mut alloc = Table::default();
        for (entry, blocks) in [("serde_json", 10.0), ("prost", 10.0), ("ciborium", 5.0)] {
            for workload in KYNOS {
                for op in ["decode", "encode"] {
                    alloc.rows.insert(
                        Key::new(DECIDING_VARIANT, entry, workload, op),
                        json!({ "allocations": blocks, "bytes": 100.0 }),
                    );
                }
            }
        }
        data.host.alloc = alloc;
        let win = frontier(&data, "ciborium");
        assert!(
            matches!(&win, Clause::Pass(why) if why.contains("decode heap blocks")),
            "{win:?}"
        );
        let gzip = AXES.iter().find(|a| a.field == "gzip").unwrap();
        data.wire
            .push(json!({ "backend": "ciborium", "workload": "json-small", "raw": 1, "gzip": 7, "zstd": 8 }));
        assert_eq!(figure(&data, gzip, "ciborium", "json-small"), Some(7.0));
    }
}
