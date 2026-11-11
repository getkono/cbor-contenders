//! Axis 4, encoded size: what each entry writes for each workload's value,
//! raw and compressed.
//!
//! The bytes come from the built `cell` (`cell dump`), so they are each
//! library's own encoding with its defaults. Each is compressed with gzip
//! level 6 (what HTTP servers default to; `miniz_oxide` through `flate2`) and
//! zstd level 3 (its default), both pinned. Sizes depend on nothing but the
//! code, so the file is host-independent, like `footprint.json`.

use std::io::Write;

use anyhow::{Context, Result};
use serde_json::json;

use crate::build::{Place, bin, build};
use crate::counted::Filter;
use crate::util::root;

/// gzip's level.
pub const GZIP_LEVEL: u32 = 6;
/// zstd's level.
pub const ZSTD_LEVEL: i32 = 3;

fn gzip(bytes: &[u8]) -> Result<usize> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(GZIP_LEVEL));
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?.len())
}

fn zstd(bytes: &[u8]) -> Result<usize> {
    Ok(zstd::bulk::compress(bytes, ZSTD_LEVEL)?.len())
}

/// `xtask wire`: raw, gzip and zstd sizes of every entry's encoding of every
/// workload it encodes, at `portable` (an encoding does not depend on the
/// CPU target).
pub fn wire(args: &[String]) -> Result<()> {
    let filter = Filter::parse(args);
    let variant = crate::matrix::variant("portable")?;
    let mut rows = Vec::new();
    for set in filter.sets() {
        let cell = bin(&build(Place::Host, "cell", set, &variant)?, "cell");
        for backend in set.backends {
            for workload in payloads::ALL {
                if crate::matrix::DECODE_ONLY.contains(workload)
                    || crate::matrix::for_workload(backend, workload, crate::matrix::SHAPES).is_empty()
                {
                    continue;
                }
                let out = std::process::Command::new(&cell)
                    .args(["dump", backend, workload])
                    .output()
                    .with_context(|| format!("cell dump {backend} {workload}"))?;
                let mut row = json!({ "set": set.name, "backend": backend, "workload": workload });
                if out.status.success() {
                    let bytes = out.stdout;
                    row["raw"] = json!(bytes.len());
                    row["gzip"] = json!(gzip(&bytes)?);
                    row["zstd"] = json!(zstd(&bytes)?);
                    row["preferred"] = json!(
                        crate::endtoend::format_of(backend) == codecs::Format::Cbor && faithful_preferred(&bytes)
                    );
                } else {
                    row["error"] = json!(String::from_utf8_lossy(&out.stderr).trim().to_owned());
                }
                rows.push(row);
            }
        }
    }
    crate::util::merge_write(
        &root().join("results/wire.json"),
        "wire",
        &rows,
        &["set", "backend", "workload"],
        json!({ "gzip_level": GZIP_LEVEL, "zstd_level": ZSTD_LEVEL }),
    )
}

/// Whether CBOR bytes are preferred serialization, by `payloads`' test of
/// the same name: the shortest head and float, definite lengths.
fn faithful_preferred(bytes: &[u8]) -> bool {
    faithful::preferred(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compressors_are_deterministic_and_smaller_on_repetition() {
        let text = b"abcabcabc".repeat(100);
        assert_eq!(gzip(&text).unwrap(), gzip(&text).unwrap());
        assert!(gzip(&text).unwrap() < text.len());
        assert!(zstd(&text).unwrap() < text.len());
    }
}
