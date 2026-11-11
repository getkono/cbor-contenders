//! Loading `results/` into lookups.
//!
//! Loading is loud: a result file that is missing or does not parse is an
//! error, unless `results/skipped.toml` declares it skipped and says why, and
//! the report states that reason. An empty table never stands in for a
//! measurement nobody took. The one exception is a `results/` with no
//! measured file at all: nothing has run yet, and the report says that and
//! nothing else ([`Data::pending`]).

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::matrix::{COUNTED_VARIANTS, DECIDING_VARIANT, SETS, SHAPES, TIMED_SHAPES, for_workload, hw_shapes};
use crate::util::{read_json, root};

/// One measurement row's identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    pub variant: String,
    pub backend: String,
    pub workload: String,
    pub op: String,
    pub form: String,
    pub arrival: String,
}

impl Key {
    pub fn of(row: &Value) -> Self {
        let s = |k: &str| row[k].as_str().unwrap_or_default().to_owned();
        Self {
            variant: s("variant"),
            backend: s("backend"),
            workload: s("workload"),
            op: s("op"),
            form: s("form"),
            arrival: s("arrival"),
        }
    }

    /// The server's own shape of `op` (owned decode of a shared frame, or
    /// `to_vec` encode).
    pub fn new(variant: &str, backend: &str, workload: &str, op: &str) -> Self {
        Self {
            variant: variant.into(),
            backend: backend.into(),
            workload: workload.into(),
            op: op.into(),
            form: "owned".into(),
            arrival: "shared".into(),
        }
    }
}

/// Rows of one kind, by key.
#[derive(Debug, Default)]
pub struct Table {
    pub rows: BTreeMap<Key, Value>,
}

impl Table {
    /// The rows of `doc`, keyed. Two rows with one key would leave one of
    /// them unread, so that is an error.
    fn of(doc: &Value) -> Result<Self> {
        let mut rows = BTreeMap::new();
        for r in doc["rows"].as_array().into_iter().flatten() {
            let key = Key::of(r);
            if rows.insert(key.clone(), r.clone()).is_some() {
                bail!("two rows for {key:?}");
            }
        }
        Ok(Self { rows })
    }

    pub fn get(&self, key: &Key, field: &str) -> Option<f64> {
        self.rows.get(key).and_then(|r| r[field].as_f64())
    }

    pub fn row(&self, key: &Key) -> Option<&Value> {
        self.rows.get(key)
    }
}

/// One result file the report read, or was told to do without.
#[derive(Debug, Clone, Default)]
pub struct Source {
    /// Path relative to `results/`.
    pub name: String,
    /// The file's provenance stamp; null when skipped.
    pub provenance: Value,
    /// Why the file is absent, from `results/skipped.toml`.
    pub skipped: Option<String>,
}

/// The per-host files.
#[derive(Debug, Default)]
pub struct Host {
    pub slug: String,
    pub alloc: Table,
    pub hw: Table,
    pub time: Table,
    pub size: Vec<Value>,
    pub compile_time: Vec<Value>,
    pub msrv: Vec<Value>,
    pub e2e_time: Vec<Value>,
    pub doctor: Value,
    /// Conformance reports by `(variant, set)`.
    pub conformance: BTreeMap<(String, String), Value>,
}

/// Everything the report reads.
#[derive(Debug, Default)]
pub struct Data {
    /// Nothing has been measured yet.
    pub pending: bool,
    pub host: Host,
    pub callgrind: Table,
    pub recount: Value,
    /// The SHA-256 of `results/callgrind/x86_64.json` as it stands, which a
    /// recount must name; `None` when it could not be read.
    pub counts_digest: Option<String>,
    pub e2e_count: Vec<Value>,
    pub footprint: Vec<Value>,
    pub advisory_db: Value,
    pub fuzz: Vec<Value>,
    pub miri: Vec<Value>,
    pub wire: Vec<Value>,
    /// `results/soundness.toml`: per crate, open and reviewed issues.
    pub soundness: toml::Table,
    /// `results/maintenance.toml`: per crate, its last release and commit.
    pub maintenance: toml::Table,
    /// Kynos's licence allow list, `references/kynos-deny-allow.txt`.
    pub licences: Vec<String>,
    /// Every file read or skipped.
    pub sources: Vec<Source>,
    /// Files neither present nor declared skipped.
    pub missing: Vec<String>,
}

/// Reads result files, recording each as a [`Source`].
struct Loader {
    results: std::path::PathBuf,
    skipped: toml::Table,
    sources: Vec<Source>,
    missing: Vec<String>,
}

impl Loader {
    fn doc(&mut self, name: &str) -> Result<Option<Value>> {
        let path = self.results.join(name);
        if !path.exists() {
            match self
                .skipped
                .get(name)
                .and_then(|s| s.get("reason"))
                .and_then(toml::Value::as_str)
            {
                Some(reason) => self.sources.push(Source {
                    name: name.to_owned(),
                    provenance: Value::Null,
                    skipped: Some(reason.to_owned()),
                }),
                None => self.missing.push(name.to_owned()),
            }
            return Ok(None);
        }
        let doc = read_json(&path).with_context(|| format!("results/{name} does not parse"))?;
        self.sources.push(Source {
            name: name.to_owned(),
            provenance: doc["provenance"].clone(),
            skipped: None,
        });
        Ok(Some(doc))
    }

    fn table(&mut self, name: &str) -> Result<Table> {
        match self.doc(name)? {
            Some(d) => Table::of(&d).with_context(|| format!("results/{name}")),
            None => Ok(Table::default()),
        }
    }

    fn rows(&mut self, name: &str) -> Result<Vec<Value>> {
        Ok(self
            .doc(name)?
            .and_then(|d| d["rows"].as_array().cloned())
            .unwrap_or_default())
    }
}

/// Files under `results/` written by hand, not by a task.
const HAND_WRITTEN: &[&str] = &["soundness.toml", "maintenance.toml", "skipped.toml"];

/// Directories under `results/` the report does not read file by file: the
/// committed fuzz corpus.
const CORPUS: &str = "fuzz-corpus";

impl Data {
    /// Load every result file.
    pub fn load() -> Result<Self> {
        Self::load_from(&root())
    }

    /// [`Self::load`] from a repository at `root`.
    pub fn load_from(root: &std::path::Path) -> Result<Self> {
        let results = root.join("results");
        let toml_file = |name: &str| -> Result<toml::Table> {
            let path = results.join(name);
            if !path.exists() {
                return Ok(toml::Table::new());
            }
            toml::from_str(&std::fs::read_to_string(&path)?).with_context(|| format!("results/{name}"))
        };
        let skipped = toml_file("skipped.toml")?;
        let hosts: Vec<String> = std::fs::read_dir(&results)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .filter(|n| n != "callgrind" && n != CORPUS)
            .collect();
        let licences = std::fs::read_to_string(root.join("references/kynos-deny-allow.txt"))
            .context("references/kynos-deny-allow.txt")?
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_owned)
            .collect();
        let mut data = Self {
            soundness: toml_file("soundness.toml")?,
            maintenance: toml_file("maintenance.toml")?,
            licences,
            ..Self::default()
        };
        let measured = std::fs::read_dir(&results)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .any(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                !HAND_WRITTEN.contains(&name.as_str()) && name != CORPUS
            });
        if !measured {
            data.pending = true;
            return Ok(data);
        }
        if hosts.len() != 1 || !hosts[0].starts_with("x86_64-linux-") {
            bail!("results/ must hold exactly one x86_64-linux-* host directory, not {hosts:?}");
        }
        let slug = hosts[0].clone();
        let mut loader = Loader {
            results: results.clone(),
            skipped,
            sources: Vec::new(),
            missing: Vec::new(),
        };
        let host_file = |name: &str| format!("{slug}/{name}");
        data.host.slug.clone_from(&slug);
        data.host.alloc = loader.table(&host_file("alloc.json"))?;
        data.host.hw = loader.table(&host_file("hw.json"))?;
        data.host.time = loader.table(&host_file("time.json"))?;
        data.host.size = loader.rows(&host_file("size.json"))?;
        data.host.compile_time = loader.rows(&host_file("compile-time.json"))?;
        data.host.msrv = loader.rows(&host_file("msrv.json"))?;
        data.host.e2e_time = loader.rows(&host_file("e2e-time.json"))?;
        data.host.doctor = loader.doc(&host_file("doctor.json"))?.unwrap_or(Value::Null);
        for variant in crate::matrix::variants() {
            for set in SETS
                .iter()
                .filter(|s| s.backends.iter().any(|b| crate::matrix::conformance_subject(b)))
            {
                let name = host_file(&format!("conformance/{}/{}.json", variant.name, set.name));
                if let Some(doc) = loader.doc(&name)? {
                    data.host
                        .conformance
                        .insert((variant.name.to_owned(), set.name.to_owned()), doc["report"].clone());
                }
            }
        }
        data.callgrind = loader.table("callgrind/x86_64.json")?;
        data.recount = loader.doc("callgrind/recount.json")?.unwrap_or(Value::Null);
        data.counts_digest = std::fs::read(results.join("callgrind/x86_64.json"))
            .ok()
            .map(|bytes| crate::counted::counts_digest(&bytes));
        data.e2e_count = loader.rows("callgrind/e2e-x86_64.json")?;
        let footprint = loader.doc("footprint.json")?.unwrap_or(Value::Null);
        data.footprint = footprint["rows"].as_array().cloned().unwrap_or_default();
        data.advisory_db = footprint["advisory_db"].clone();
        data.fuzz = loader.rows("fuzz.json")?;
        data.miri = loader.rows("miri.json")?;
        data.wire = loader.rows("wire.json")?;
        unread(&results, &loader.sources, &slug)?;
        data.sources = loader.sources;
        data.missing = loader.missing;
        Ok(data)
    }
}

/// Refuse a file under `results/` the report does not read: it would be a
/// measurement nobody sees.
fn unread(results: &std::path::Path, sources: &[Source], slug: &str) -> Result<()> {
    for entry in walkdir::WalkDir::new(results) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(results)?.to_string_lossy().replace('\\', "/");
        if HAND_WRITTEN.contains(&rel.as_str()) || rel.starts_with(&format!("{CORPUS}/")) {
            continue;
        }
        if !sources.iter().any(|s| s.name == rel) {
            bail!("results/{rel} is not read by the report (host {slug})");
        }
    }
    Ok(())
}

/// Sources measured on other code than `fingerprint`, on another host, or
/// from a dirty checkout.
pub fn stale(sources: &[Source], fingerprint: Option<&str>, host: &str) -> Vec<String> {
    sources
        .iter()
        .filter(|s| s.skipped.is_none())
        .filter_map(|s| {
            let p = &s.provenance;
            let theirs = p["fingerprint"].as_str();
            if p["git_dirty"] != false {
                Some(format!("{}: measured from a dirty checkout", s.name))
            } else if theirs.is_none() || theirs != fingerprint {
                Some(format!(
                    "{}: measured on code {} (HEAD is {})",
                    s.name,
                    theirs.unwrap_or("without a fingerprint"),
                    fingerprint.unwrap_or("unknown")
                ))
            } else if p["host"].as_str() != Some(host) {
                Some(format!(
                    "{}: measured on host {} (results are for {host})",
                    s.name,
                    p["host"].as_str().unwrap_or("without a host stamp"),
                ))
            } else {
                None
            }
        })
        .collect()
}

/// A per-operation table to check: its file, rows, variants, shapes per
/// backend and workload, and whether the baselines have A/A copies.
type PerOperation<'a> = (
    &'a str,
    &'a Table,
    &'a [&'a str],
    &'a dyn Fn(&str, &str) -> Vec<crate::matrix::Shape>,
    bool,
);

/// A recount that does not vouch for every count committed now: one that
/// compared fewer than the rows with a count (a rejected, unsupported or
/// wrong cell has none, so it is neither committed nor compared), at a
/// tolerance above zero, or of another counts file than this one.
fn recount_gaps(data: &Data) -> Vec<String> {
    let r = &data.recount;
    if r.is_null() {
        return Vec::new(); // missing or skipped: the loader said so
    }
    let mut gaps = Vec::new();
    let counted = data.callgrind.rows.values().filter(|r| r["ir"].is_number()).count();
    if r["tolerance"].as_f64() != Some(0.0)
        || r["compared"].as_u64() != Some(counted as u64)
        || r["committed"].as_u64() != Some(counted as u64)
        || counted == 0
    {
        gaps.push(format!(
            "callgrind/recount.json: {} of {} counts recounted at tolerance {}, against {counted} counts committed now; the check needs every one of them at 0",
            r["compared"], r["committed"], r["tolerance"]
        ));
    }
    match r["counts_sha256"].as_str() {
        None => gaps.push("callgrind/recount.json: names no counts file (`counts_sha256`); rerun `mise run count:check`".into()),
        Some(stamp) if Some(stamp) != data.counts_digest.as_deref() => gaps.push(format!(
            "callgrind/recount.json: verified the counts file with SHA-256 {stamp}, but callgrind/x86_64.json now has {}; rerun `mise run count:check`",
            data.counts_digest.as_deref().unwrap_or("no digest (it could not be read)")
        )),
        Some(_) => {}
    }
    gaps
}

/// Whether `name` (under `results/`) was read: present, not skipped. A
/// file that is missing or skipped is the loader's to report; one that is
/// present is checked for every row it should hold, even when it holds none.
fn read(data: &Data, name: &str) -> bool {
    data.sources.iter().any(|s| s.name == name && s.skipped.is_none())
}

/// What a complete `mise run all` would have written and did not.
#[must_use]
pub fn incomplete(data: &Data) -> Vec<String> {
    if data.pending {
        return Vec::new();
    }
    let mut gaps: Vec<String> = data
        .missing
        .iter()
        .map(|m| format!("results/{m}: missing (and not declared in results/skipped.toml)"))
        .collect();
    gaps.extend(per_operation_gaps(data));
    gaps.extend(recount_gaps(data));
    gaps.extend(doctor_gaps(data));
    for ((variant, set), report) in &data.host.conformance {
        if let Some(e) = report["error"].as_str() {
            gaps.push(format!("conformance/{variant}/{set}: the suite did not finish ({e})"));
        }
        for (name, entry) in report["backends"].as_object().into_iter().flatten() {
            if let Some(e) = entry["error"].as_str() {
                gaps.push(format!(
                    "conformance/{variant}/{set}: the {name} suite did not finish ({e})"
                ));
            }
        }
    }
    gaps.extend(per_set_gaps(data));
    gaps.extend(wire_gaps(data));
    gaps.extend(end_to_end_gaps(data));
    gaps.extend(fuzz_and_miri_gaps(data));
    gaps
}

/// The per-operation tables' missing and failed rows, counted per file
/// with the first named.
fn per_operation_gaps(data: &Data) -> Vec<String> {
    let at = |f: &str| format!("{}/{f}", data.host.slug);
    let per_op: [PerOperation<'_>; 4] = [
        (
            "callgrind/x86_64.json",
            &data.callgrind,
            COUNTED_VARIANTS,
            &|b, w| for_workload(b, w, SHAPES),
            false,
        ),
        (
            "alloc.json",
            &data.host.alloc,
            &[DECIDING_VARIANT],
            &|b, w| for_workload(b, w, SHAPES),
            false,
        ),
        ("hw.json", &data.host.hw, &["portable", "native"], &hw_shapes, true),
        (
            "time.json",
            &data.host.time,
            &[DECIDING_VARIANT],
            &|b, w| for_workload(b, w, TIMED_SHAPES),
            true,
        ),
    ];
    let mut gaps = Vec::new();
    for (name, table, variants, shapes, aa) in per_op {
        let file = if name.starts_with("callgrind/") {
            name.to_owned()
        } else {
            at(name)
        };
        if !read(data, &file) {
            continue;
        }
        let workloads: Vec<&str> = if name == "time.json" {
            crate::timed::default_workloads()
                .into_iter()
                .filter_map(|w| payloads::ALL.iter().copied().find(|a| *a == w))
                .collect()
        } else {
            payloads::ALL.to_vec()
        };
        let (mut missing, mut failed, mut first) = (0usize, 0usize, None);
        for variant in variants {
            for set in SETS {
                for backend in set.backends {
                    let mut labels = vec![(*backend).to_owned()];
                    if aa && set.name == "baseline" {
                        labels.push(format!("{backend}#aa"));
                    }
                    for label in labels {
                        for workload in &workloads {
                            for shape in shapes(backend, workload) {
                                let key = Key {
                                    variant: (*variant).to_owned(),
                                    backend: label.clone(),
                                    workload: (*workload).to_owned(),
                                    op: shape.op.to_owned(),
                                    form: shape.form.to_owned(),
                                    arrival: shape.arrival.to_owned(),
                                };
                                // A wrong or refused result is a result
                                // (clause 3 reads it); an error is a cell
                                // that did not run.
                                let what = match table.row(&key) {
                                    None => "missing",
                                    Some(row) if row["error"].is_string() => "failed",
                                    Some(_) => continue,
                                };
                                if what == "missing" {
                                    missing += 1;
                                } else {
                                    failed += 1;
                                }
                                first.get_or_insert_with(|| format!("{what}: {key:?}"));
                            }
                        }
                    }
                }
            }
        }
        if missing + failed > 0 {
            gaps.push(format!(
                "{name}: {missing} rows missing, {failed} failed (first {})",
                first.unwrap_or_default()
            ));
        }
    }
    gaps
}

/// Doctor's failures, and a doctor that did not check every measured build
/// at every variant.
fn doctor_gaps(data: &Data) -> Vec<String> {
    let doctor = &data.host.doctor;
    if doctor.is_null() {
        return Vec::new();
    }
    let mut gaps = Vec::new();
    if let Some(f) = doctor["failures"].as_array().filter(|f| !f.is_empty()) {
        gaps.push(format!(
            "doctor.json: {} builds not compiled as their variant claims (first: {})",
            f.len(),
            f[0]
        ));
    }
    let builds = doctor["builds"].as_array().map_or(0, Vec::len);
    let expected = crate::matrix::variants().len() * crate::matrix::measured_builds().len();
    if builds != expected {
        gaps.push(format!(
            "doctor.json: {builds} builds checked, not every measured build at every variant ({expected})"
        ));
    }
    gaps
}

/// The per-set files' missing rows, and the hand-written reviews' gaps.
fn per_set_gaps(data: &Data) -> Vec<String> {
    let at = |f: &str| format!("{}/{f}", data.host.slug);
    let mut gaps = Vec::new();
    let variants: Vec<&str> = crate::matrix::variants().iter().map(|v| v.name).collect();
    for (file, rows, per_variant) in [
        (at("size.json"), &data.host.size, true),
        (at("compile-time.json"), &data.host.compile_time, false),
        (at("msrv.json"), &data.host.msrv, false),
        ("footprint.json".to_owned(), &data.footprint, false),
    ] {
        if !read(data, &file) {
            continue;
        }
        let mut lacking = Vec::new();
        for set in SETS {
            for (krate, _) in set.costs {
                let wanted: &[&str] = if per_variant { &variants } else { &[""] };
                for v in wanted {
                    let found = rows
                        .iter()
                        .any(|r| r["set"] == set.name && r["crate"] == *krate && (v.is_empty() || r["variant"] == *v));
                    if !found {
                        lacking.push(format!("{} {krate} {v}", set.name).trim().to_owned());
                    }
                }
            }
        }
        if !lacking.is_empty() {
            gaps.push(format!("{file}: no row for {}", lacking.join(", ")));
        }
    }
    for set in SETS {
        for (krate, _) in set.costs {
            // Clause 4 reads the hand review of open issues; a crate
            // missing from it would pass unreviewed.
            if data
                .soundness
                .get(*krate)
                .and_then(|t| t.get("open"))
                .and_then(toml::Value::as_array)
                .is_none()
            {
                gaps.push(format!("soundness.toml: no `open` list for {krate}"));
            }
            if set.name != "baseline" && !data.maintenance.contains_key(*krate) {
                gaps.push(format!("maintenance.toml: no entry for {krate}"));
            }
        }
    }
    gaps
}

/// The entries and workloads `wire.json` has no size for.
fn wire_gaps(data: &Data) -> Vec<String> {
    if !read(data, "wire.json") {
        return Vec::new();
    }
    let mut lacking = Vec::new();
    for set in SETS {
        for backend in set.backends {
            for workload in payloads::ALL {
                if crate::matrix::DECODE_ONLY.contains(workload) || for_workload(backend, workload, SHAPES).is_empty() {
                    continue;
                }
                // An entry that cannot encode a value has a row saying so.
                if !data
                    .wire
                    .iter()
                    .any(|r| r["set"] == set.name && r["backend"] == *backend && r["workload"] == *workload)
                {
                    lacking.push(format!("{backend} {workload}"));
                }
            }
        }
    }
    if lacking.is_empty() {
        Vec::new()
    } else {
        vec![format!(
            "wire.json: {} sizes missing (first {})",
            lacking.len(),
            lacking[0]
        )]
    }
}

/// The servers and routes an end-to-end file lacks, or whose run failed:
/// every entry and floor, at its variants, with the baselines' A/A copies
/// where the run is timed.
fn end_to_end_gaps(data: &Data) -> Vec<String> {
    let mut gaps = Vec::new();
    let timed = format!("{}/e2e-time.json", data.host.slug);
    for (name, rows, variants, aa) in [
        ("callgrind/e2e-x86_64.json", &data.e2e_count, COUNTED_VARIANTS, false),
        (timed.as_str(), &data.host.e2e_time, &[DECIDING_VARIANT][..], true),
    ] {
        if !read(data, name) {
            continue;
        }
        let mut lacking = Vec::new();
        for v in variants {
            for set in SETS {
                for backend in crate::endtoend::backends(set) {
                    let mut labels = vec![backend.to_owned()];
                    if aa && set.name == "baseline" && ["serde_json", "prost"].contains(&backend) {
                        labels.push(format!("{backend}#aa"));
                    }
                    for b in labels {
                        for route in crate::endtoend::ROUTES {
                            let row = rows
                                .iter()
                                .find(|r| r["variant"] == *v && r["backend"] == b.as_str() && r["route"] == route.0);
                            // A wrong answer is a result, which clause 3 reads.
                            if row.is_none_or(|r| r["error"].is_string() && r["verdict"] != "wrong") {
                                lacking.push(format!("{v} {b} {}", route.0));
                            }
                        }
                    }
                }
            }
        }
        if !lacking.is_empty() {
            gaps.push(format!(
                "{name}: {} rows missing or failed (first {})",
                lacking.len(),
                lacking[0]
            ));
        }
    }
    gaps
}

/// Fuzz runs that are missing or did not finish, and sets Miri has no
/// result for or could not run on.
fn fuzz_and_miri_gaps(data: &Data) -> Vec<String> {
    let mut gaps = Vec::new();
    if read(data, "fuzz.json") {
        for set in SETS {
            for run in crate::fuzzing::runs(set) {
                let found = data
                    .fuzz
                    .iter()
                    .find(|r| r["backend"] == run.1 && r["config"] == run.2 && r["target"] == run.4);
                match found {
                    None => gaps.push(format!("fuzz.json: no run for {} {} {}", run.1, run.2, run.4)),
                    Some(r) if r["finished"] != true => {
                        gaps.push(format!("fuzz.json: {} {} {} did not finish", run.1, run.2, run.4));
                    }
                    Some(_) => {}
                }
            }
        }
    }
    if read(data, "miri.json") {
        for set in SETS
            .iter()
            .filter(|s| s.backends.iter().any(|b| crate::matrix::conformance_subject(b)))
        {
            match data.miri.iter().find(|r| r["set"] == set.name) {
                None => gaps.push(format!("miri.json: no run for {}", set.name)),
                Some(r) if r["outcome"] == "error" => {
                    gaps.push(format!("miri.json: Miri itself failed for {}", set.name));
                }
                Some(_) => {}
            }
        }
    }
    gaps
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{Data, Key, Table, recount_gaps};

    fn source(name: &str) -> super::Source {
        super::Source {
            name: name.to_owned(),
            ..super::Source::default()
        }
    }

    /// A file that is present is checked even when it holds no rows: an
    /// empty table is no stand-in for a measurement nobody took.
    #[test]
    fn a_present_file_with_no_rows_is_checked() {
        let host = "x86_64-linux-h";
        let mut data = Data::default();
        data.host.slug = host.into();
        for name in [
            "wire.json".to_owned(),
            "miri.json".to_owned(),
            "fuzz.json".to_owned(),
            "callgrind/e2e-x86_64.json".to_owned(),
            format!("{host}/e2e-time.json"),
            format!("{host}/size.json"),
            format!("{host}/hw.json"),
        ] {
            data.sources.push(source(&name));
        }
        data.host.doctor = json!({ "failures": [], "builds": [] });
        let gaps = super::incomplete(&data).join("\n");
        for want in [
            "wire.json:",
            "miri.json: no run for ciborium",
            "fuzz.json: no run for",
            "callgrind/e2e-x86_64.json:",
            "e2e-time.json:",
            "size.json: no row for",
            "hw.json:",
            "doctor.json: 0 builds checked",
            "soundness.toml: no `open` list for ciborium",
        ] {
            assert!(gaps.contains(want), "no gap {want:?} in:\n{gaps}");
        }
    }

    /// The timed end-to-end file needs every floor and both A/A copies.
    #[test]
    fn the_timed_end_to_end_file_needs_its_copies_and_floors() {
        let host = "x86_64-linux-h";
        let mut data = Data::default();
        data.host.slug = host.into();
        data.sources.push(source(&format!("{host}/e2e-time.json")));
        for set in crate::matrix::SETS {
            for backend in crate::endtoend::backends(set) {
                for route in crate::endtoend::ROUTES {
                    data.host
                        .e2e_time
                        .push(json!({ "variant": "portable", "backend": backend, "route": route.0 }));
                }
            }
        }
        let gaps = super::end_to_end_gaps(&data);
        assert!(gaps.len() == 1 && gaps[0].contains("#aa"), "{gaps:?}");
        for b in ["serde_json#aa", "prost#aa"] {
            for route in crate::endtoend::ROUTES {
                data.host
                    .e2e_time
                    .push(json!({ "variant": "portable", "backend": b, "route": route.0 }));
            }
        }
        assert!(super::end_to_end_gaps(&data).is_empty());
    }

    /// Three counted rows and two that hold no count: a refusal and an
    /// unsupported shape.
    fn counts() -> Table {
        let mut t = Table::default();
        for (workload, row) in [
            ("json-small", json!({ "verdict": "exact", "ir": 10.0 })),
            ("echo-post", json!({ "verdict": "exact", "ir": 20.0 })),
            ("json-large", json!({ "verdict": "inexact", "ir": 30.0 })),
            ("nested-10000", json!({ "verdict": "rejected", "detail": "depth" })),
            ("enums", json!({ "verdict": "unsupported" })),
        ] {
            t.rows
                .insert(Key::new("portable", "serde_json", workload, "decode"), row);
        }
        t
    }

    fn data(recount: Value) -> Data {
        Data {
            callgrind: counts(),
            recount,
            counts_digest: Some("aaaa".into()),
            ..Data::default()
        }
    }

    /// Rejected and unsupported rows have no count, so a recount of every
    /// count compared three, not five.
    #[test]
    fn a_recount_of_every_count_passes_beside_rows_without_one() {
        let ok = json!({ "compared": 3, "committed": 3, "tolerance": 0.0, "counts_sha256": "aaaa" });
        assert_eq!(recount_gaps(&data(ok)), Vec::<String>::new());
    }

    #[test]
    fn a_recount_short_of_the_counts_or_above_zero_tolerance_is_a_gap() {
        for bad in [
            json!({ "compared": 2, "committed": 3, "tolerance": 0.0, "counts_sha256": "aaaa" }),
            json!({ "compared": 5, "committed": 5, "tolerance": 0.0, "counts_sha256": "aaaa" }),
            json!({ "compared": 3, "committed": 3, "tolerance": 0.005, "counts_sha256": "aaaa" }),
        ] {
            assert_eq!(recount_gaps(&data(bad.clone())).len(), 1, "{bad}");
        }
    }

    #[test]
    fn a_recount_of_another_counts_file_is_a_gap() {
        let other = json!({ "compared": 3, "committed": 3, "tolerance": 0.0, "counts_sha256": "bbbb" });
        let gaps = recount_gaps(&data(other));
        assert!(gaps.len() == 1 && gaps[0].contains("now has aaaa"), "{gaps:?}");
        let unstamped = json!({ "compared": 3, "committed": 3, "tolerance": 0.0 });
        assert_eq!(recount_gaps(&data(unstamped)).len(), 1);
        assert!(recount_gaps(&data(Value::Null)).is_empty());
    }
}
