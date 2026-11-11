//! Section `appendix_a`: the 81 examples of RFC 8949 Appendix A (Table 6).
//!
//! Each row is decoded into the type its diagnostic notation names
//! (`references/rfc8949/appendix_a.toml`) and must yield that value. A row
//! refused, or decoded to another value, gates, except the rows whose answer
//! the RFC leaves to the implementation (bignums, `undefined`, unassigned
//! simple values, tags), whose outcome is recorded. Every decoded row is
//! re-encoded by the same library, and whether the bytes are the RFC's is
//! recorded (preferred serialization is section `deterministic`'s to judge).
//!
//! The transcription is verified before any row runs: its digest, the RFC
//! text's digest, and every row's diagnostic notation and encoding against
//! the table in the vendored RFC text, in order.

use std::path::{Path, PathBuf};

use codecs::Backend;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::cases::{decode, hex};
use crate::outcome::{Outcome, guarded};
use crate::record::Section;

/// One row of Table 6, with the harness's reading of it.
#[derive(Debug, Clone, Deserialize)]
pub struct Row {
    /// The RFC's diagnostic notation.
    pub diagnostic: String,
    /// The RFC's encoding, in hex.
    pub hex: String,
    /// What it is decoded into (`cases::path`).
    pub target: String,
    /// The value, as JSON text; absent for `None`.
    pub expect: Option<String>,
    /// Whether a refusal or another value gates.
    #[serde(default = "yes")]
    pub gate: bool,
    /// Why not, where it does not.
    pub why: Option<String>,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct Table {
    row: Vec<Row>,
}

/// Where the vendored RFC and its transcription live.
#[must_use]
pub fn default_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../references/rfc8949")
}

/// Load and verify the transcription in `dir`.
///
/// # Errors
/// A file missing or moved from its digest, or a row that is not the RFC's.
pub fn load(dir: &Path) -> Result<Vec<Row>, String> {
    let read = |name: &str| std::fs::read(dir.join(name)).map_err(|e| format!("{name}: {e}"));
    let sums = String::from_utf8(read("SHA256SUMS")?).map_err(|e| e.to_string())?;
    for line in sums.lines() {
        let (digest, name) = line
            .split_once("  ")
            .ok_or_else(|| format!("bad SHA256SUMS line: {line}"))?;
        let actual = hex::encode(Sha256::digest(read(name)?));
        if actual != digest {
            return Err(format!("{name} is not the vendored file: sha256 {actual}"));
        }
    }
    let rfc = String::from_utf8(read("rfc8949.txt")?).map_err(|e| e.to_string())?;
    let table: Table = toml::from_str(&String::from_utf8(read("appendix_a.toml")?).map_err(|e| e.to_string())?)
        .map_err(|e| format!("appendix_a.toml: {e}"))?;
    let printed = table_6(&rfc)?;
    if printed.len() != table.row.len() {
        return Err(format!(
            "Table 6 has {} rows, the transcription {}",
            printed.len(),
            table.row.len()
        ));
    }
    for ((diagnostic, encoded), row) in printed.iter().zip(&table.row) {
        if *diagnostic != row.diagnostic || *encoded != row.hex {
            return Err(format!(
                "row {:?} is not the RFC's {diagnostic:?} = {encoded}",
                row.diagnostic
            ));
        }
    }
    Ok(table.row)
}

/// Table 6's rows as the RFC text prints them: a row spans the lines between
/// two `+---` rules, its cells joined.
fn table_6(rfc: &str) -> Result<Vec<(String, String)>, String> {
    let start = rfc
        .find("   |Diagnostic                    | Encoded")
        .ok_or("no Table 6 header")?;
    let end = rfc
        .find("Table 6: Examples of Encoded CBOR Data Items")
        .ok_or("no Table 6 caption")?;
    let mut rows = Vec::new();
    let mut current: Option<(String, String)> = None;
    for line in rfc[start..end].lines().skip(2) {
        if line.starts_with("   +") {
            rows.extend(current.take());
            continue;
        }
        let Some(cells) = line.strip_prefix("   |") else {
            continue;
        };
        let mut parts = cells.split('|');
        let (Some(left), Some(right)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (left, right) = (left.trim_end(), right.trim());
        match &mut current {
            None => current = Some((left.trim().to_owned(), right.to_owned())),
            Some((diagnostic, encoded)) => {
                if !left.trim().is_empty() {
                    diagnostic.push(' ');
                    diagnostic.push_str(left.trim());
                }
                encoded.push_str(right);
            }
        }
    }
    Ok(rows
        .into_iter()
        .map(|(diagnostic, encoded)| (diagnostic, encoded.trim_start_matches("0x").to_owned()))
        .collect())
}

/// Whether a decoded value is the expected one.
fn matches(expected: Option<&str>, got: &Value) -> bool {
    let Some(expected) = expected else {
        return got.is_null();
    };
    let Ok(expected) = serde_json::from_str::<Value>(expected) else {
        return false;
    };
    crate::outcome::same_value(&expected, got) || numerically_equal(&expected, got)
}

/// Integers compared by value across `u64`/`i64` representations (a
/// `Value` decoded from CBOR holds `1` as either).
fn numerically_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            (x.is_i64() || x.is_u64()) && (y.is_i64() || y.is_u64()) && x.to_string() == y.to_string()
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|(x, y)| crate::outcome::same_value(x, y) || numerically_equal(x, y))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter().all(|(k, v)| {
                    y.get(k)
                        .is_some_and(|w| crate::outcome::same_value(v, w) || numerically_equal(v, w))
                })
        }
        _ => false,
    }
}

/// Decode `bytes` into `target` and re-encode the value with `B`.
fn reencode<B: Backend>(target: &str, bytes: &[u8]) -> Option<Vec<u8>> {
    fn again<B: Backend, T: serde::de::DeserializeOwned + serde::Serialize>(bytes: &[u8]) -> Option<Vec<u8>> {
        let value: T = B::decode(bytes::Bytes::copy_from_slice(bytes)).ok()?;
        B::encode(&value).ok()
    }
    guarded(|| match target {
        "u64" => again::<B, u64>(bytes),
        "i64" => again::<B, i64>(bytes),
        "f64" => again::<B, f64>(bytes),
        "bool" => again::<B, bool>(bytes),
        "option" => again::<B, Option<u8>>(bytes),
        "string" => again::<B, String>(bytes),
        "bytes" => again::<B, serde_bytes::ByteBuf>(bytes),
        "value" => again::<B, Value>(bytes),
        "int-map" => again::<B, std::collections::BTreeMap<u64, u64>>(bytes),
        _ => None,
    })
    .ok()
    .flatten()
}

/// Run the section.
#[must_use]
pub fn run<B: Backend>(rows: &[Row]) -> Section {
    let mut section = Section::new("appendix_a");
    let mut recorded = Vec::new();
    for row in rows {
        let bytes = hex(&row.hex);
        let case = format!("{} = {}", row.diagnostic, row.hex);
        section.case();
        let outcome = decode::<B>(&row.target, &bytes);
        let verdict = match &outcome {
            Outcome::Accept(got) if matches(row.expect.as_deref(), got) => "value",
            Outcome::Accept(_) => "other value",
            Outcome::Reject(_) => "refused",
            Outcome::Panic(_) => "panicked",
        };
        if verdict != "value" {
            section.differ(
                &case,
                json!({ "target": row.target, "expect": row.expect }),
                outcome.describe(),
            );
            if row.gate || verdict == "panicked" || verdict == "other value" && row.target != "ignored" {
                section.fail(&case, format!("{verdict} into {}", row.target));
            }
        }
        let reencoded = (verdict == "value")
            .then(|| reencode::<B>(&row.target, &bytes))
            .flatten();
        recorded.push(json!({
            "diagnostic": row.diagnostic,
            "hex": row.hex,
            "target": row.target,
            "gates": row.gate,
            "outcome": verdict,
            "detail": outcome.describe(),
            "reencoded": reencoded.as_ref().map(hex::encode),
            "reencoded_same": reencoded.as_deref().map(|again| again == bytes.as_slice()),
        }));
    }
    section.data("rows", Value::Array(recorded));
    section
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_transcription_is_the_rfcs_table() {
        let rows = load(&default_dir()).expect("verifies");
        assert_eq!(rows.len(), 81);
        assert_eq!(rows[0].hex, "00");
        assert_eq!(rows[80].diagnostic, r#"{_ "Fun": true, "Amt": -2}"#);
    }

    #[test]
    fn the_oracle_reads_the_rows_it_must() {
        let rows = load(&default_dir()).expect("verifies");
        let section = run::<codecs::backend::serde_cbor::SerdeCbor>(&rows);
        let (value, failures) = section.finish();
        // serde_cbor has no half-precision NaN payload issue, reads bignums
        // only with its `tags` feature (off here), and so on: whatever it
        // refuses must be a row that does not gate, and nothing that gates
        // may fail.
        assert!(failures.is_empty(), "{failures:?}\n{value}");
    }
}
