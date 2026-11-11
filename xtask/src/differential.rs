//! The conformance suite, run over the build matrix.

use std::process::Command;

use anyhow::Result;
use serde_json::json;

use crate::build::{Place, bin, build_with};
use crate::counted::Filter;
use crate::util::{capture, root, write_json};

/// `xtask conformance`: every candidate set at every variant (code
/// generation differs by variant, so a result at one is not one at the
/// other). The baseline set has no CBOR candidate and is not run.
pub fn conformance(args: &[String]) -> Result<()> {
    let filter = Filter::parse(args);
    let host = crate::provenance::host_slug();
    let mut failures = 0;
    for variant in filter.variants() {
        for set in filter
            .sets()
            .into_iter()
            .filter(|s| s.backends.iter().any(|b| crate::matrix::conformance_subject(b)))
        {
            {
                let release = build_with(Place::Host, "conformance", set, &variant, None)?;
                let name = set.name.to_owned();
                eprintln!("conformance: {} {name}", variant.name);
                let (ok, stdout, stderr) = capture(&mut Command::new(bin(&release, "conform")))?;
                // A run fails if it printed no report, printed an error
                // report (a corpus that failed verification), or exited
                // non-zero; whatever it printed is still written, with an
                // `error` the report reads.
                let mut report = serde_json::from_str::<serde_json::Value>(stdout.trim())
                    .ok()
                    .filter(serde_json::Value::is_object)
                    .unwrap_or_else(|| json!({ "error": "conform produced no report", "stdout": stdout }));
                if !ok && !report["error"].is_string() {
                    report["error"] = json!("conform exited non-zero");
                }
                if report["error"].is_string() {
                    failures += 1;
                    report["stderr"] = json!(stderr);
                }
                let path = root()
                    .join("results")
                    .join(&host)
                    .join("conformance")
                    .join(variant.name)
                    .join(format!("{name}.json"));
                write_json(
                    &path,
                    &json!({ "schema": 1, "provenance": crate::provenance::stamp_checked()?, "report": report }),
                )?;
            }
        }
    }
    anyhow::ensure!(
        failures == 0,
        "{failures} conformance runs failed or produced no report"
    );
    Ok(())
}
