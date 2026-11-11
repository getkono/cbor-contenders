# cbor-contenders

A server that takes typed JSON bodies can take CBOR (RFC 8949) as well: the same `T: Serialize + Deserialize`, another serde format. This repository asks what that costs and what it breaks.

- Which Rust CBOR library decodes and encodes a typed body fastest, with the least heap, in the fewest bytes? Measured against the formats a server already has: JSON through serde_json and protobuf through prost.
- Does each one answer every body correctly? That means RFC 8949's well-formedness and validity rules, serde's derive attributes, and every integer and float bit for bit.
- What does the JSON data model, which OpenAPI describes every body through, lose when the body is CBOR? Every library is checked on each case.

It is the harness kynos [#989](https://github.com/getkono/kynos/issues/989) specifies, delivered as [#999](https://github.com/getkono/kynos/issues/999). The repository also holds the worked outside codec #988 makes possible: a `BodyCodec` for `application/cbor` that Kynos mounts through `Encoded<C, T>`, written to the codec trust model and tested through Kynos ([`crates/kynos-cbor`](crates/kynos-cbor/src/lib.rs)).

This README holds the method and the commentary. Every measured number, table and figure is generated from `results/` into [`report/`](report/README.md) by `cargo xtask report`; nothing in this file is a measurement the report does not show. The infrastructure is getkono/serde-json-contenders', taken at `7c4e50f` and adapted: workspace, provenance, container counts, timed protocol, footprint and report checks.

## Contents

- [Findings](#findings)
- [Scope](#scope)
- [Methodology](#methodology): candidates, what a backend must do, builds, axes, workloads, conformance, the JSON data model, end to end, the decision rule, verification
- [Where this departs from #989](#where-this-departs-from-989)
- [Using the worked codec](#using-the-worked-codec)
- [Reproduce](#reproduce)
- [Licence](#licence)

## Findings

Pending the first measurement: `results/` holds no measured file yet, so the [report](report/README.md#verdict) shows only the decision rule's first clause. That clause is settled outside this repository: kynos #998 found no written use case that an outside `BodyCodec` cannot serve, so CBOR is not admitted in-tree whatever the measurements show. The rest of the rule is computed from the results once `mise run all` has run on a quiet host.

## Scope

This harness measures x86-64 Linux, and only that, for the same reason as serde-json-contenders. Its instruments are Linux's: `perf_event_open` for hardware cycles, `/proc/schedstat` and `sched_setaffinity` for pinning and for rejecting disturbed samples, `taskset` for the end-to-end server, and callgrind for deterministic counts. Its builds are x86-64 builds. #989 also names aarch64 and Intel; there is no such host to measure on, so neither is measured.

## Methodology

### Candidates

Every version is pinned exactly in the root `Cargo.toml`. Each was re-read on 2026-11-10/11 from crates.io, the crate's repository and RustSec, as #989 asks, and not taken from the issue.

| Crate | Version | Licence | Role | Entry points |
| --- | --- | --- | --- | --- |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | baseline (`Json<T>`) | `from_slice`, `to_vec`, `to_writer` |
| prost (+ prost-derive) | 0.14.4 | Apache-2.0 | baseline (`Protobuf<T>`) | `Message::decode` on the body's `Bytes`, `encode_to_vec`, `encode`. 0.14.4 is the version Kynos's lockfile resolves (0.14.5 was released on 2026-11-10) |
| ciborium | 0.2.2 | Apache-2.0 | candidate | `from_reader`, `into_writer` |
| minicbor-serde (minicbor 2.3.1, `std`, `half`) | 0.7.2 | BlueOak-1.0.0 | candidate | `from_slice`, `to_vec`, `Serializer` |
| minicbor, own derive | 2.3.1 | BlueOak-1.0.0 | reference point | `decode`, `to_vec`, `encode` on `#[derive(Encode, Decode)]` types |
| cbor4ii (`serde1`, `use_std`, `half-f16`) | 1.2.4 | MIT | candidate, not in #989 | `serde::from_slice`, `to_vec`, `to_writer` |
| cbor2 (`std`) | 1.1.6 | MIT | candidate, not in #989 | `from_slice`, `to_vec`, `to_writer` |
| c2pa_cbor | 0.78.2 | MIT OR Apache-2.0 | candidate, not in #989 | `from_slice`, `to_vec`, `to_writer` |
| serde_cbor | 0.11.2 | MIT OR Apache-2.0 | oracle, never measured | `from_slice`, `to_vec` |

Notes on the table:

- **The three added candidates.** cbor4ii is maintained, borrows from the input, and is widely used. cbor2 is new (first published 2026-06-12) and claims well-formedness checks, limits and canonical encoding, claims only a measurement can settle. c2pa_cbor is a maintained descendant of serde_cbor's API with a deterministic mode.
- **minicbor's derive** is measured as #989 asks ("measure both"). It is a reference point, not a candidate, for two reasons. It needs a second derive on every body type. Its default wire form, a struct as an array indexed by `#[n]`, is not what a JSON Schema object describes, so no schema Kynos derives would be true of it.
- **serde_cbor** is unmaintained (RUSTSEC-2021-0127; repository archived). It is here as the decoder none of the candidates shares code with. Every CBOR input is its `to_vec`, which is preferred serialization (RFC 8949 section 4.1); `payloads` tests that, so no decoder is measured on an encoding a typical client would not send. Every candidate's output is read back through it, and conformance compares against it.

Each adapter uses its library's defaults. No adapter adds a check its library lacks: a trailing-bytes check, a depth limit, a deterministic mode. Conformance judges the library as a server would get it, and `Backend::TRAILING` records which ones check for trailing bytes.

### Not measured, and why

Each was checked on 2026-11-10:

- **serde_cbor_2** (0.13.0): kanidm's fork of the oracle, with the same API.
- **serde_ipld_dagcbor** (0.7.0): DAG-CBOR, a strict subset of CBOR, built on cbor4ii.
- **cbor-smol** (0.5.1): a heapless CTAP subset that writes through its own writer, not `std::io::Write`.
- **serde_cbor_core** (0.1.0): a single release with no adoption.
- **cbor_event** and **dcbor**: not serde.
- **coset**: COSE, built on ciborium.
- **cbor-codec**: no release since 2017.
- **cbor** (BurntSushi): marked deprecated and unmaintained by its author.

### What a backend must do

The contract is what a server's typed body does (`crates/codecs`):

- Decode `T` from the `Bytes` the server received, owned or borrowing from the body.
- Encode `T` into a fresh `Vec`, into a reused buffer, or into the `std::io::Write` a framework hands a codec (Kynos's `BodyCodec::encode`).
- Classify each failure. The line is RFC 8949's own: bytes that are not a valid CBOR item (ill-formed, section 5.1, or invalid, section 5.3) are the client's syntax, a 400. A valid item of the wrong shape for `T` is a 422.

Each adapter maps its library's errors as faithfully as the error type allows, and says in a comment what it cannot distinguish. minicbor-serde's error hides its kind, so on the failure path only its adapter asks minicbor's own decoder whether the body is well-formed. Any copy a backend needs to meet the contract happens inside the adapter, so it is counted.

### Builds

- **One set per candidate.** Each candidate crate is built in a set of its own, always beside the baselines and the oracle. A build in which a crate every set shares (serde_json, prost, bytes) resolves other features than the baseline build does is refused.
- **Profile and compiler.** Every build uses Kynos's profile (`lto = "fat"`, `codegen-units = 1`, `panic = "unwind"`, line tables only) on Rust 1.97.1, the toolchain Kynos's `mise.toml` pins.
- **Variants.** There are two: `portable` (`-C target-cpu=x86-64`), which decides because #989's rule asks for the frontier "at a portable build", and `native`, for information. No CBOR candidate selects a SIMD path, so there is no intermediate level to tell apart. `cargo xtask doctor` checks that each build was compiled for its variant's target features.

### Axes

| # | Axis (#989 section 2) | Instrument |
| --- | --- | --- |
| 1 | Decode CPU | hardware `cycles:u` at `portable`, pinned, in shuffled rounds with an A/A copy of each baseline: this decides. callgrind estimated cycles (`Ir + 5·L1 misses + 35·LL misses`, fixed cache geometry) at the same build corroborate it and reproduce exactly. Wall-clock time is recorded for information |
| 2 | Encode CPU | the same |
| 3 | Heap | a counting global allocator in a build of its own: allocations, reallocations, bytes, peak per operation |
| 4 | Encoded size | each entry's own encoding of each workload's value: raw, gzip level 6, zstd level 3 |
| 5 | `.text` | a fixture that decodes and re-encodes the two Kynos body types, minus the same program without a codec. It mounts the codec and the types, not a Kynos server: every CBOR codec reaches Kynos through the same `Encoded<C, T>`, so Kynos's per-codec glue is the same for each |
| 6 | Compile time and MSRV | clean and incremental builds of that fixture (median of five; adoption context, not an axis), and whether each adapter builds on Rust 1.85, Kynos's MSRV |
| 7 | `unsafe` and soundness | a `syn`-based count of every `unsafe` block, fn, impl and trait in every runtime crate a candidate adds (serde-json-contenders' replacement for cargo-geiger); RustSec advisories at a pinned advisory-db commit; open soundness issues reviewed by hand (`results/soundness.toml`) |
| 8 | Conformance | the suite below: a gate, not a trade-off |

The measurement protocol (verification before measurement, warm-up, iteration counts, pinning, SMT-sibling and interference checks, rounds, A/A control, trust classes) is serde-json-contenders', unchanged. A cell whose result is wrong is reported and never measured.

### Workloads

Every workload is a typed Rust struct, never an untyped tree, in an owned form and a borrowing form. All inputs are generated from a fixed splitmix64 stream and pinned by SHA-256 per format (`crates/payloads/SHA256SUMS`).

| Group | Workloads |
| --- | --- |
| Kynos shapes (the decision rule) | `json-small`, `echo-post`, `json-large`: kynos-bench's values, whose JSON is byte-identical to its golden bodies (asserted by digest) |
| Size sweep | `Vec<Item>` at exactly 64 B to 1 MiB of CBOR |
| Families | `ints`, `ints-extremes`, `floats`, `f32s`, `floats-short`, `unicode`, `long-string`, `nested-{100,255,257,10000}` (either side of the 256 limit ciborium, cbor4ii and cbor2 share; serde_json stops at 128), `wide-1000`, `array-small`, `enums`, `sparse`, `defaults`, `keys-long-int` (integer map keys, which CBOR writes as integers and JSON as strings), and two CBOR families: `byte-strings` (opaque blobs, a byte string in CBOR and base64 in JSON) and `numeric-arrays` |
| Real-world schemas | `twitter-like`, `citm-like`, `canada-like`, `skip-unknown` |
| Rejections | `reject-truncated` (`json-large` cut at 90 %: a 400) and `reject-shape` (one `id` as text: a 422) |

Notes:

- **Dropped workloads.** serde-json-contenders' JSON-text workloads (escapes, pretty-printing, `\u` escapes) have no CBOR meaning and are dropped.
- **Protobuf and minicbor's derive** carry the workloads whose shape they can. A root list becomes a one-field message, since a protobuf message cannot be a bare list. Every other workload is not measured for them, and the matrix says so.
- **Arrivals.** Decode input arrives as one frame sharing a larger buffer (`shared`, as hyper hands out a single-frame body) or as a fresh buffer (`unique`, a collected multi-frame body).

### Conformance

The suite is `crates/conformance`. Its references are RFC 8949's text where the RFC answers, serde_json on the same value written as JSON where serde's derive decides the answer, and the serde_cbor oracle elsewhere. **Silent differences gate**:

- a body accepted that the RFC refuses;
- a value other than the input's;
- a status of the other class;
- a crash;
- an allocation of the declared size over a short body.

Answers RFC 8949 leaves to the implementation are recorded, not gated: duplicate map keys, tags, bignums, simple values, depth limits, encoding choices.

| Section | What | Gates |
| --- | --- | --- |
| `appendix_a` | The 81 examples of RFC 8949 Appendix A, transcribed from the vendored RFC (`references/rfc8949`). The transcription is checked row by row against the RFC text. Each row is decoded into its natural type and re-encoded | a refusal or another value, except bignums, `undefined`, unassigned simple values and tags |
| `wellformed` | section 5.1's ill-formed bodies, into a value and skipped, both arrivals | accepting one, or answering 422 |
| `strings` | invalid UTF-8 in definite and indefinite text, in a skipped member, into `String`, `&str`, `Cow` | accepting it, or answering 422 |
| `structs` | `deny_unknown_fields`, ill-formed skipped members, missing members; repeated keys recorded | yes / recorded |
| `depth` | 100 up to 1 000 000 levels, in a subprocess, on 2 MiB and 8 MiB stacks | a crash, or answering 422 |
| `lengths` | heads declaring 2^32 and 2^63 items or bytes over a few-byte body, under a counting allocator | allocating more than 1 MiB, or accepting it |
| `integers` | every type's boundaries; `-2^64` and bignums recorded | another value, an out-of-range value accepted, the wrong class |
| `floats` | half, single and double; `-0`; infinities; subnormals; NaN payloads recorded; a million random `f64` and `f32` through the library's own encoder and decoder, bit for bit | yes |
| `deterministic` | the same value encoded twice; preferred serialization per workload, and a `HashMap` across processes, recorded | differing bytes in one process |
| `attributes` | `flatten`, `untagged`, three enum taggings, `alias`, `rename_all`, `default`, `skip`, `Option`, borrowing, against serde_json on the same value | yes |
| `model` | the JSON data model (next section) | the worked codec only |
| `status` | every truncation of the three Kynos bodies, and a wrong shape | yes |
| `encode_diff` | every workload encoded and decoded, against the oracle and the workload's own value | another value |

Beyond the suite:

- **Fuzzing.** Differential fuzzing (cargo-fuzz, `nightly-2026-09-25`, AddressSanitizer), 30 minutes per target and candidate, on two workers, continuing past every divergence. There are three targets:
  - arbitrary bytes into a generic value and into `IgnoredAny`;
  - arbitrary bytes into an attribute-heavy struct, owned and borrowed;
  - arbitrary values round-tripped.

  A divergence is an ill-formed body accepted, as judged by `faithful::well_formed`, which shares no code with any candidate. A value that differs from the oracle's is one too. The minimized corpus of every run is committed under `results/fuzz-corpus/`.
- **Miri** runs a slice of the suite wherever a candidate's code allows. Where it cannot run (`unsupported`), clause 3 stays pending.

### The JSON data model (#989 section 5)

OpenAPI 3.2 reads a non-JSON body through the JSON Schema data model, and Kynos describes an `Encoded<C, T>` body with the schema it derives from `T`. CBOR carries values that model has no place for. Kynos's trust model puts the duty on the codec: refuse what the schema cannot describe. The `model` section records, for every library bare, what it does with each such case:

- byte strings into `Vec<u8>` and `serde_bytes`, and arrays of integers into them;
- integer map keys;
- tags 0 to 5 and 55799;
- `undefined` and simple values;
- NaN and infinities;
- integers beyond 2^53 and 2^64;
- indefinite lengths;
- non-preferred heads.

It also records what the worked codec answers. The worked codec's rules, in `crates/faithful`:

| Body | Answer | Why |
| --- | --- | --- |
| ill-formed (section 5.1), invalid UTF-8, bytes after the item, nested deeper than 128 | 400 | not a valid CBOR item; 128 is the depth `Json<T>` refuses past |
| a byte string | 422 | JSON has no bytes; Kynos describes `Vec<u8>` as an array of integers, and a `contentEncoding: base64` string is text, not a byte string |
| any tag | 422 | no JSON Schema form; a decoder that ignored it would accept a value the schema does not describe |
| `undefined`, simple values other than `false`, `true`, `null` | 422 | no JSON value |
| NaN, ±infinity | 422 | not JSON numbers |
| a non-text map key | 422 | JSON Schema object keys are strings (Kynos derives no schema for a `BTreeMap<u64, _>`) |
| a repeated key in one map | 422 | invalid under RFC 8949 section 5.6, and an object has one value per key |
| indefinite lengths, non-preferred heads and floats, any 64-bit integer | accepted | transparent to the schema |

On encode, a byte string, a non-finite float, an `i128`/`u128` and a non-string map key are refused, which Kynos answers 500. The cost of the scan is inside every end-to-end figure for a CBOR entry.

### End to end

`crates/e2e` is a Kynos server (kynos 0.5.0 from crates.io) with kynos-bench's four routes: `POST /echo`, `GET /json/large`, `POST /json/large` (204) and `GET /json/small`. kynos-bench itself cannot swap the codec under a route, and is private. Each entry carries its bodies the way an application would:

- `Json<T>` for serde_json;
- `Protobuf<T>` for prost;
- for each CBOR candidate, `Encoded<Cbor<B>, T>`: the worked codec, scan included.

Each format has a floor: the same server reading `Binary<M>` under the same media type and answering with pre-encoded bytes. The codec's share of a request is (entry − floor) ÷ entry.

- **Counted** runs go through `Server::serve_io` over an in-memory pipe under callgrind, at `portable`.
- **Timed** runs go over loopback with oha 1.16.0. Closed-loop throughput is taken at 64 connections, then open-loop p99 at 70 % of serde_json's throughput. Five reps run in shuffled order, with A/A copies of both baselines.

#989 section 6's cross-check is the report's: a large timed gain where the counted share is small is a measurement error to explain, not a result.

### Decision rule (#989 section 7)

CBOR is admitted in-tree only if all five hold, per candidate (`xtask/src/report/verdict.rs`):

1. **A written use case an outside `BodyCodec` cannot serve.** Settled by kynos #998: there is none. The report states it and computes nothing.
2. **The frontier.**
   - **The win.** On at least one of axes 1 to 4, at a Kynos workload and the server's own shape (owned decode of a shared frame, `to_vec` encode), the candidate is more than the margin better than both serde_json and prost, at `portable`.
   - **The margin.** It is ε = 5 %, or on a cycles axis the A/A band at that cell where wider.
   - **Counts must agree.** A cycles advantage counts only where callgrind's estimated cycles at the same build point the same way.
3. **No silent conformance difference.** The suite's gate at every variant, and no wrong answer in any measured cell or end-to-end route. There must also be no fuzz divergence and no undefined behaviour under Miri. The worked codec's profile must hold on every `model` case. minicbor's derive fails here by construction: it has no faithful schema.
4. **Soundness.** No open soundness issue, and no advisory affecting the pinned version of any crate a candidate adds. The crate must also be maintained: a release or substantive commit within twelve months of `results/maintenance.toml`'s date.
5. **Licence.** The licence of every runtime crate a candidate adds must be on Kynos's `deny.toml` allow list, vendored as `references/kynos-deny-allow.txt`. BlueOak-1.0.0 (minicbor) is not on it, as #989's outcome comment records.

Each clause is pass, fail or pending, with its reason, in the report and `report/verdict.json`.

### Verification

1. **The harness is correct.** Run `mise run test`, once per set. It covers payload digests and preferred serialization, every adapter, the worked codec through Kynos for every library, the conformance harness, the verdict, and that the measurements can fail (`crates/cell/src/sensitivity.rs`). The last needs valgrind, so it runs in the container: `mise run sensitivity`.
2. **The results are one measurement of the committed code.** Every result file is stamped with a fingerprint of the measured code. The report refuses a file stamped with another fingerprint, from another host, or from a dirty checkout. It also refuses a file under `results/` it does not read, and a matrix row that is missing.
3. **The counts reproduce.** `mise run count:check` recounts every committed count from an empty `target/` at zero tolerance and records the digest of the counts it checked.
4. **The commentary matches the report.** Every number under [Findings](#findings) links to the report row it comes from, and `cargo xtask report --check` fails if that row does not hold it.

## Where this departs from #989

- **Hosts.** x86-64 Linux on one AMD Zen 4 host only. There is no aarch64 or Intel host, so clause 2's "and on aarch64" is not measured.
- **Variants.** `portable` decides clause 2, as #989 asks. Unlike serde-json-contenders, `native` does not decide; it is reported.
- **Appendix A is transcribed.** The vectors are taken from the vendored RFC (a Code Component, Revised BSD), not from `cbor/test-vectors`. That repository carries no licence, its vectors are RFC 7049's, and it includes `0xf818`, which RFC 8949 makes ill-formed.
- **`.text`.** It is measured on a fixture holding the codec and the body types, not on a Kynos application (see Axes).
- **The candidate list.** Three crates the issue does not name were added; minicbor's derive is a reference point, not a candidate; prost is pinned to Kynos's lockfile. All are covered under Candidates.
- **DHAT.** Heap is counted by a global allocator, as in serde-json-contenders: blocks and bytes per operation, exactly.

## Using the worked codec

[`crates/kynos-cbor`](crates/kynos-cbor/src/lib.rs) is a `BodyCodec` for `application/cbor` over any library the harness has, held to the rules above:

```rust
use kynos::extract::body::codec::Encoded;
use kynos_cbor::CiboriumCbor;

#[kynos::post("/items")]
async fn create(item: Encoded<CiboriumCbor, Item>) -> Encoded<CiboriumCbor, Item> {
    item
}
```

The crate's example serves it, negotiated beside `Json<T>`:

```sh
cargo run -p kynos-cbor --features ciborium --example serve
```

Its tests (`crates/kynos-cbor/tests/end_to_end.rs`) drive every row of its table through Kynos for every library: 415, 400, 422, 500, and `Accept`.

## Reproduce

On an x86-64 Linux host with podman (or docker) and mise, from a clean checkout:

```sh
mise install                          # toolchain, nextest, convco, oha, hk, cargo-fuzz
CC_TRUST=canonical mise run all       # on a quieted host (every CPU at `performance`, load ≤ 1.0); hours
git add results && git commit -m "chore(results): ..."
cargo xtask report                    # regenerate report/ from results/
cargo xtask report --check
```

`mise run all` without `CC_TRUST=canonical` takes `solo`-trust timing on a busy host, which the report marks provisional. Individual tasks are listed by `cargo xtask`. A stage that cannot run on a host is declared, with its reason, in `results/skipped.toml`, and the report states it.

## Licence

MIT or Apache-2.0, at your option. The vendored RFC 8949 text and its Table 6 transcription are under the IETF Trust's terms (see `references/rfc8949/LICENSE`).
