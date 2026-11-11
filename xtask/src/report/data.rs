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

/// What a complete `mise run all` would have written and did not.
#[must_use]
#[expect(clippy::too_many_lines, reason = "one check per result file, in order")]
pub fn incomplete(data: &Data) -> Vec<String> {
    if data.pending {
        return Vec::new();
    }
    let mut gaps: Vec<String> = data
        .missing
        .iter()
        .map(|m| format!("results/{m}: missing (and not declared in results/skipped.toml)"))
        .collect();
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
    for (name, table, variants, shapes, aa) in per_op {
        if table.rows.is_empty() {
            continue; // missing or skipped: said above
        }
        let workloads: Vec<&str> = if name == "time.json" {
            crate::timed::default_workloads()
                .into_iter()
                .filter_map(|w| payloads::ALL.iter().copied().find(|a| *a == w))
                .collect()
        } else {
            payloads::ALL.to_vec()
        };
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
                                match table.row(&key) {
                                    None => gaps.push(format!("{name}: no row for {key:?}")),
                                    Some(row) if row["error"].is_string() => {
                                        gaps.push(format!("{name}: {key:?} failed: {}", row["error"]));
                                    }
                                    Some(_) => {}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if !data.callgrind.rows.is_empty() {
        let committed = data.callgrind.rows.len() as u64;
        if data.recount["compared"].as_u64() != Some(committed) || data.recount["tolerance"].as_f64() != Some(0.0) {
            gaps.push(format!(
                "callgrind/recount.json: the zero-tolerance recount did not compare every one of the {committed} counts"
            ));
        }
        let counts = std::fs::read(root().join("results/callgrind/x86_64.json")).unwrap_or_default();
        let digest = crate::counted::counts_digest(&counts);
        if data.recount["counts_sha256"].as_str() != Some(digest.as_str()) {
            gaps.push("callgrind/recount.json: recounted another counts file than the committed one".into());
        }
    }
    if !data.host.doctor.is_null() && data.host.doctor["failures"].as_array().is_some_and(|f| !f.is_empty()) {
        gaps.push("doctor.json: doctor reported failures".into());
    }
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
    for set in SETS {
        for (krate, _) in set.costs {
            for (file, rows) in [
                ("size.json", &data.host.size),
                ("compile-time.json", &data.host.compile_time),
                ("msrv.json", &data.host.msrv),
                ("footprint.json", &data.footprint),
            ] {
                if !rows.is_empty() && !rows.iter().any(|r| r["crate"] == *krate) {
                    gaps.push(format!("{file}: no row for {krate}"));
                }
            }
            if !data.soundness.contains_key(*krate) {
                gaps.push(format!("soundness.toml: no entry for {krate}"));
            }
            if set.name != "baseline" && !data.maintenance.contains_key(*krate) {
                gaps.push(format!("maintenance.toml: no entry for {krate}"));
            }
        }
        for entry in crate::matrix::e2e_entries(set) {
            for route in crate::endtoend::ROUTES {
                if !data.e2e_count.is_empty()
                    && !data
                        .e2e_count
                        .iter()
                        .any(|r| r["backend"] == entry && r["route"] == route.0)
                {
                    gaps.push(format!("e2e-x86_64.json: no row for {entry} {}", route.0));
                }
                if !data.host.e2e_time.is_empty()
                    && !data
                        .host
                        .e2e_time
                        .iter()
                        .any(|r| r["backend"] == entry && r["route"] == route.0)
                {
                    gaps.push(format!("e2e-time.json: no row for {entry} {}", route.0));
                }
            }
        }
        if !data.fuzz.is_empty() {
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
    gaps
}
