//! Proof that each measured build was compiled for the target its variant
//! names, on this CPU.
//!
//! A `native` build is only a native build if the compiler saw the CPU's
//! features, and a `portable` one only portable if it saw none beyond
//! x86-64's. No CBOR candidate selects a SIMD path of its own, so there is no
//! path to check beyond that. Every measured build is checked, not only the
//! `cell` that answers for itself: the conformance suite, the end-to-end
//! server and the size fixture too. A run on a machine where the check fails
//! is refused.

use std::process::Command;

use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::build::{Place, bin, build};
use crate::matrix::variants;
use crate::util::{output, root, write_json};

/// `rustc --print cfg` target features for a `RUSTFLAGS` string.
fn rustc_features(rustflags: &str) -> Result<Vec<String>> {
    let mut cmd = Command::new("rustc");
    cmd.current_dir(root())
        .args(["--print", "cfg"])
        .args(rustflags.split_whitespace());
    Ok(output(&mut cmd)?
        .lines()
        .filter_map(|l| l.strip_prefix("target_feature=\""))
        .filter_map(|l| l.strip_suffix('"'))
        .map(str::to_owned)
        .collect())
}

/// Refuse to measure unless `doctor` passed on this host for this code. A
/// dirty checkout is let through: its results are drafts the report refuses,
/// so nothing it measures is accepted by the report anyway.
pub fn require() -> Result<()> {
    let stamp = crate::provenance::stamp();
    if stamp["git_dirty"] != false {
        eprintln!("doctor: the checkout is dirty, so these results are drafts; doctor is not required");
        return Ok(());
    }
    let path = root()
        .join("results")
        .join(crate::provenance::host_slug())
        .join("doctor.json");
    let doc = crate::util::read_json(&path)
        .map_err(|_| anyhow::anyhow!("no doctor result: run `cargo xtask doctor` first"))?;
    anyhow::ensure!(
        doc["provenance"]["fingerprint"] == stamp["fingerprint"] && doc["provenance"]["git_dirty"] == false,
        "doctor ran on other code ({}); run `cargo xtask doctor` first",
        doc["provenance"]["fingerprint"]
    );
    let failures = doc["failures"].as_array().map_or(0, Vec::len);
    anyhow::ensure!(
        failures == 0,
        "doctor found {failures} builds that were not compiled as their variant claims"
    );
    Ok(())
}

/// The target features `cell doctor` probes: every x86-64 feature a
/// compiler may auto-vectorize or select instructions with, up to AVX-512.
pub const PROBED: &[&str] = &[
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
    "avx512bw",
];

/// Probed features where `compiled` differs from what rustc enables for the
/// variant (`expected`): one missing, or one beyond it.
fn feature_mismatches(expected: &[String], compiled: &[String]) -> Vec<String> {
    PROBED
        .iter()
        .filter_map(|f| {
            let want = expected.iter().any(|e| e == f);
            let got = compiled.iter().any(|c| c == f);
            (want != got).then(|| format!("target feature {f} expected {want}, compiled {got}"))
        })
        .collect()
}

/// The `RUSTFLAGS` cargo compiled `bin` of `package` with, from the build's
/// own fingerprint (the newest, where the directory holds several).
fn compiled_rustflags(release: &std::path::Path, package: &str, bin: &str) -> Option<Vec<String>> {
    let prefix = format!("{package}-");
    let file = format!("bin-{bin}.json");
    let newest = std::fs::read_dir(release.join(".fingerprint"))
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .map(|e| e.path().join(&file))
        .filter_map(|p| Some((p.metadata().ok()?.modified().ok()?, p)))
        .max_by_key(|(t, _)| *t)?
        .1;
    let fingerprint: Value = serde_json::from_str(&std::fs::read_to_string(newest).ok()?).ok()?;
    fingerprint["rustflags"]
        .as_array()
        .map(|flags| flags.iter().filter_map(|f| f.as_str().map(str::to_owned)).collect())
}

/// `xtask doctor`: build every measured build ([`crate::matrix::measured_builds`])
/// at every variant on this host, and check each was compiled for its
/// variant. A `cell` is asked what it compiled, and every probed feature
/// must be exactly the variant's. Any other build is checked from cargo's
/// own record of the flags it compiled each measured binary with, which
/// must be the variant's: the `cell` checks prove those flags give the
/// variant's features.
pub fn doctor(_args: &[String]) -> Result<()> {
    let mut report = Vec::new();
    let mut failures = Vec::new();
    for variant in variants() {
        let expected = rustc_features(variant.rustflags)?;
        let flags: Vec<String> = variant.rustflags.split_whitespace().map(str::to_owned).collect();
        for b in crate::matrix::measured_builds() {
            let release = if b.feature.is_empty() {
                build(Place::Host, b.package, b.set, &variant)?
            } else {
                crate::build::build_featured(Place::Host, b.package, b.set, &variant, b.feature)?
            };
            let what = format!(
                "{} {} {}{}",
                variant.name,
                b.set.name,
                b.package,
                if b.feature.is_empty() {
                    String::new()
                } else {
                    format!(" +{}", b.feature)
                }
            );
            let mut entry = json!({
                "variant": variant.name, "set": b.set.name, "package": b.package, "feature": b.feature,
                "rustc_features": expected,
            });
            if b.package == "cell" {
                let text = output(Command::new(bin(&release, "cell")).arg("doctor"))?;
                let doc: Value = serde_json::from_str(text.trim())?;
                let compiled: Vec<String> = doc["compiled_features"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
                failures.extend(
                    feature_mismatches(&expected, &compiled)
                        .into_iter()
                        .map(|m| format!("{what}: {m}")),
                );
                entry["cell"] = doc;
            }
            let mut compiled_flags = serde_json::Map::new();
            for name in b.bins {
                let got = compiled_rustflags(&release, b.package, name);
                if got.as_ref() != Some(&flags) {
                    failures.push(format!(
                        "{what}: {name} compiled with RUSTFLAGS {got:?}, not the variant's {flags:?}"
                    ));
                }
                compiled_flags.insert((*name).to_owned(), json!(got));
            }
            entry["compiled_rustflags"] = Value::Object(compiled_flags);
            report.push(entry);
        }
    }
    let path = root()
        .join("results")
        .join(crate::provenance::host_slug())
        .join("doctor.json");
    write_json(
        &path,
        &json!({ "schema": 2, "kind": "doctor", "provenance": crate::provenance::stamp_checked()?, "failures": failures, "builds": report }),
    )?;
    if failures.is_empty() {
        eprintln!(
            "doctor: every build compiled as its variant claims ({})",
            path.display()
        );
        Ok(())
    } else {
        for f in &failures {
            eprintln!("doctor: {f}");
        }
        bail!("{} doctor failures", failures.len())
    }
}

#[cfg(test)]
mod tests {
    use super::{PROBED, feature_mismatches};

    fn list(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    /// Every probed feature is checked both ways: missing, or beyond the
    /// variant.
    #[test]
    fn every_probed_feature_is_checked() {
        let all = list(PROBED);
        assert!(feature_mismatches(&all, &all).is_empty());
        for f in PROBED {
            let without: Vec<String> = all.iter().filter(|x| x != f).cloned().collect();
            assert_eq!(feature_mismatches(&all, &without).len(), 1, "{f} missing");
            assert_eq!(feature_mismatches(&without, &all).len(), 1, "{f} beyond");
        }
    }

    /// `cell doctor`'s probe list is this one.
    #[test]
    fn the_probe_lists_agree() {
        let cell = include_str!("../../crates/cell/src/doctor.rs");
        let start = cell.find("probe!(").unwrap();
        let end = start + cell[start..].find(");").unwrap();
        let probed: Vec<&str> = cell[start..end].split('"').skip(1).step_by(2).collect();
        assert_eq!(probed, PROBED);
    }
}
