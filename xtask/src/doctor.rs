//! Proof that each build was compiled for the target its variant names, on
//! this CPU.
//!
//! A `native` build is only a native build if the compiler saw the CPU's
//! features, and a `portable` one only portable if it saw none beyond
//! x86-64's. No CBOR candidate selects a SIMD path of its own, so there is no
//! path to check beyond that. A run on a machine where the check fails is
//! refused.

use std::process::Command;

use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::build::{Place, bin, build};
use crate::matrix::{SETS, variants};
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

/// `xtask doctor`: build every set at every variant on this host, ask each
/// `cell` what it compiled, and check it.
pub fn doctor(_args: &[String]) -> Result<()> {
    let mut report = Vec::new();
    let mut failures = Vec::new();
    for variant in variants() {
        let expected = rustc_features(variant.rustflags)?;
        for set in SETS {
            let release = build(Place::Host, "cell", set, &variant)?;
            let text = output(Command::new(bin(&release, "cell")).arg("doctor"))?;
            let doc: Value = serde_json::from_str(text.trim())?;
            let compiled: Vec<String> = doc["compiled_features"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
            // Every probed feature rustc enables for this variant must be
            // compiled in, and nothing it does not enable.
            let probed = [
                "sse2",
                "ssse3",
                "sse4.1",
                "sse4.2",
                "avx2",
                "bmi2",
                "pclmulqdq",
                "avx512f",
            ];
            for f in probed {
                let want = expected.iter().any(|e| e == f);
                let got = compiled.iter().any(|c| c == f);
                if want != got {
                    failures.push(format!(
                        "{} {}: target feature {f} expected {want}, compiled {got}",
                        variant.name, set.name
                    ));
                }
            }

            report.push(json!({ "variant": variant.name, "set": set.name, "rustc_features": expected, "cell": doc }));
        }
    }
    let path = root()
        .join("results")
        .join(crate::provenance::host_slug())
        .join("doctor.json");
    write_json(
        &path,
        &json!({ "schema": 1, "kind": "doctor", "provenance": crate::provenance::stamp_checked()?, "failures": failures, "builds": report }),
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
