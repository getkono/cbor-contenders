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

/// One axis of clause 2: what it reads, and whether lower is better (always).
#[derive(Debug, Clone, Copy)]
pub struct Axis {
    pub name: &'static str,
    pub op: &'static str,
    /// `cycles` (hw), `bytes` (alloc), `raw` (wire).
    pub field: &'static str,
}

/// Axes 1 to 4.
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
        name: "decode heap bytes",
        op: "decode",
        field: "bytes",
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
];

/// An entry's figure on `axis` at `workload` (portable, the server's shape).
pub fn figure(data: &Data, axis: &Axis, entry: &str, workload: &str) -> Option<f64> {
    let key = Key::new(DECIDING_VARIANT, entry, workload, axis.op);
    match axis.field {
        "cycles" => data.host.hw.get(&key, "cycles"),
        "bytes" => data.host.alloc.get(&key, "bytes"),
        _ => data
            .wire
            .iter()
            .find(|r| r["backend"] == entry && r["workload"] == workload)
            .and_then(|r| r["raw"].as_f64()),
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

/// Clause 2 for `entry`.
fn frontier(data: &Data, entry: &str) -> Clause {
    let mut pending = Vec::new();
    let mut wins = Vec::new();
    for workload in KYNOS {
        for axis in AXES {
            let Some(mine) = figure(data, axis, entry, workload) else {
                pending.push(format!("{} at {workload}", axis.name));
                continue;
            };
            let mut beats_both = true;
            for baseline in ["serde_json", "prost"] {
                let (Some(theirs), Some(band)) = (
                    figure(data, axis, baseline, workload),
                    aa_band(data, axis, baseline, workload),
                ) else {
                    pending.push(format!("{} at {workload} ({baseline})", axis.name));
                    beats_both = false;
                    continue;
                };
                let margin = EPSILON.max(band);
                let better = mine < theirs * (1.0 - margin);
                let agrees = axis.field != "cycles"
                    || matches!(
                        (counted(data, axis, entry, workload), counted(data, axis, baseline, workload)),
                        (Some(m), Some(t)) if m < t
                    );
                beats_both &= better && agrees;
            }
            if beats_both {
                wins.push(format!("{} at {workload}", axis.name));
            }
        }
    }
    if !wins.is_empty() {
        Clause::Pass(format!(
            "better than serde_json and prost beyond the margin on {}",
            wins.join(", ")
        ))
    } else if !pending.is_empty() {
        Clause::Pending(format!("not measured: {}", pending.join(", ")))
    } else {
        Clause::Fail(
            "on no axis 1–4 at a Kynos workload better than both serde_json and prost beyond the margin".into(),
        )
    }
}

/// Clause 3 for `entry` of `set`.
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
    let wrong = |rows: &mut dyn Iterator<Item = &Value>| {
        rows.filter(|r| r["backend"] == entry && r["verdict"] == "wrong")
            .count()
    };
    let measured = wrong(&mut data.callgrind.rows.values())
        + wrong(&mut data.host.alloc.rows.values())
        + wrong(&mut data.host.hw.rows.values());
    if measured > 0 {
        failures.push(format!("{measured} measured cells answered wrongly"));
    }
    let e2e = wrong(&mut data.e2e_count.iter()) + wrong(&mut data.host.e2e_time.iter());
    if e2e > 0 {
        failures.push(format!("{e2e} end-to-end routes answered wrongly"));
    }
    if crate::matrix::conformance_subject(entry) {
        let runs: Vec<&Value> = data.fuzz.iter().filter(|r| r["backend"] == entry).collect();
        if runs.is_empty() {
            pending.push("fuzzing".into());
        }
        let diverged = runs.iter().filter(|r| r["divergence_found"] == true).count();
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
    let mut failures = Vec::new();
    match data
        .soundness
        .get(krate)
        .and_then(|e| e.get("open"))
        .and_then(toml::Value::as_array)
    {
        Some(open) if !open.is_empty() => failures.push(format!("{} open soundness issues", open.len())),
        Some(_) => {}
        None => return Clause::Pending("no soundness.toml entry".into()),
    }
    let Some(row) = data.footprint.iter().find(|r| r["set"] == set && r["crate"] == krate) else {
        return Clause::Pending("no footprint".into());
    };
    let advisories: Vec<String> = row["crates"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|c| c["advisories"].as_array().into_iter().flatten().map(move |a| (c, a)))
        .filter(|(_, a)| a["affects_pinned_version"] == true && a["informational"].is_null())
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
    match maintained(data, krate) {
        Some(true) => {}
        Some(false) => failures.push("no release or substantive commit within twelve months".into()),
        None => return Clause::Pending("no maintenance.toml entry".into()),
    }
    if failures.is_empty() {
        Clause::Pass("no open soundness issue or advisory, and maintained".into())
    } else {
        Clause::Fail(failures.join("; "))
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

/// Whether an SPDX expression is acceptable under `allowed`: an `OR` (or
/// `/`) takes any side, an `AND` every side.
pub fn licence_ok(expression: &str, allowed: &[String]) -> bool {
    expression.split(" OR ").flat_map(|p| p.split('/')).any(|alternative| {
        alternative.split(" AND ").all(|l| {
            allowed
                .iter()
                .any(|a| a == l.trim().trim_matches(|c| c == '(' || c == ')'))
        })
    })
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
                    alloc.rows.insert(key, json!({ "bytes": bytes }));
                }
            }
        }
        data.host.hw = hw;
        data.callgrind = cg;
        data.host.alloc = alloc;
        for entry in ["serde_json", "prost", "ciborium"] {
            for workload in KYNOS {
                data.wire
                    .push(json!({ "backend": entry, "workload": workload, "raw": 100 }));
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
}
