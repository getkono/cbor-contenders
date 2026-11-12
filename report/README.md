# Results

Nothing has been measured yet: `results/` holds no measured file. Run `mise run all` on a quiet x86-64 Linux host (`CC_TRUST=canonical mise run all` once it is quieted), commit `results/`, and run `cargo xtask report`.

<a id="verdict"></a>

## Verdict (kynos #989 section 7)

CBOR is admitted in-tree only if every clause holds. Clause 1 was settled by kynos #998 before anything here ran; the rest are computed from the results.

| Candidate | Admitted | 1 use case | 2 frontier | 3 conformance | 4 soundness | 5 licence |
| --- | --- | --- | --- | --- | --- | --- |
| `ciborium` | no | fail | pending | pending | fail | pending |
| `minicbor-serde` | no | fail | pending | pending | pending | pending |
| `minicbor` | no | fail | pending | fail | pending | pending |
| `cbor4ii` | no | fail | pending | pending | pending | pending |
| `cbor2` | no | fail | pending | pending | pending | pending |
| `c2pa_cbor` | no | fail | pending | pending | pending | pending |

<a id="clauses"></a>

## The clauses, entry by entry

| Candidate | Clause | Outcome | Why |
| --- | --- | --- | --- |
| `ciborium` | 1 | fail | kynos #998: no written use case an outside BodyCodec cannot serve |
| `ciborium` | 2 | pending | not measured: decode cycles at json-small, encode cycles at json-small, decode heap blocks at json-small, decode heap bytes at json-small, encode heap blocks at json-small, encode heap bytes at json-small, encoded size at json-small, encoded size gzip at json-small, encoded size zstd at json-small, decode cycles at echo-post, encode cycles at echo-post, decode heap blocks at echo-post, decode heap bytes at echo-post, encode heap blocks at echo-post, encode heap bytes at echo-post, encoded size at echo-post, encoded size gzip at echo-post, encoded size zstd at echo-post, decode cycles at json-large, encode cycles at json-large, decode heap blocks at json-large, decode heap bytes at json-large, encode heap blocks at json-large, encode heap bytes at json-large, encoded size at json-large, encoded size gzip at json-large, encoded size zstd at json-large |
| `ciborium` | 3 | pending | not measured: conformance at portable, conformance at native, no callgrind cells, no alloc cells, no hw cells, no time cells, no e2e-count routes, no e2e-time routes, fuzzing decode_value, fuzzing decode_struct, fuzzing roundtrip, Miri |
| `ciborium` | 4 | fail | no release or code change within twelve months |
| `ciborium` | 5 | pending | no footprint |
| `minicbor-serde` | 1 | fail | kynos #998: no written use case an outside BodyCodec cannot serve |
| `minicbor-serde` | 2 | pending | not measured: decode cycles at json-small, encode cycles at json-small, decode heap blocks at json-small, decode heap bytes at json-small, encode heap blocks at json-small, encode heap bytes at json-small, encoded size at json-small, encoded size gzip at json-small, encoded size zstd at json-small, decode cycles at echo-post, encode cycles at echo-post, decode heap blocks at echo-post, decode heap bytes at echo-post, encode heap blocks at echo-post, encode heap bytes at echo-post, encoded size at echo-post, encoded size gzip at echo-post, encoded size zstd at echo-post, decode cycles at json-large, encode cycles at json-large, decode heap blocks at json-large, decode heap bytes at json-large, encode heap blocks at json-large, encode heap bytes at json-large, encoded size at json-large, encoded size gzip at json-large, encoded size zstd at json-large |
| `minicbor-serde` | 3 | pending | not measured: conformance at portable, conformance at native, no callgrind cells, no alloc cells, no hw cells, no time cells, no e2e-count routes, no e2e-time routes, fuzzing decode_value, fuzzing decode_struct, fuzzing roundtrip, Miri |
| `minicbor-serde` | 4 | pending | not run to its end: fuzzing minicbor-serde portable decode_value, fuzzing minicbor-serde portable decode_struct, fuzzing minicbor-serde portable roundtrip; no footprint |
| `minicbor-serde` | 5 | pending | no footprint |
| `minicbor` | 1 | fail | kynos #998: no written use case an outside BodyCodec cannot serve |
| `minicbor` | 2 | pending | not measured: decode cycles at json-small, encode cycles at json-small, decode heap blocks at json-small, decode heap bytes at json-small, encode heap blocks at json-small, encode heap bytes at json-small, encoded size at json-small, encoded size gzip at json-small, encoded size zstd at json-small, decode cycles at echo-post, encode cycles at echo-post, decode heap blocks at echo-post, decode heap bytes at echo-post, encode heap blocks at echo-post, encode heap bytes at echo-post, encoded size at echo-post, encoded size gzip at echo-post, encoded size zstd at echo-post, decode cycles at json-large, encode cycles at json-large, decode heap blocks at json-large, decode heap bytes at json-large, encode heap blocks at json-large, encode heap bytes at json-large, encoded size at json-large, encoded size gzip at json-large, encoded size zstd at json-large |
| `minicbor` | 3 | fail | its wire form (a struct as an array) has no faithful JSON Schema (section 5) |
| `minicbor` | 4 | pending | no footprint |
| `minicbor` | 5 | pending | no footprint |
| `cbor4ii` | 1 | fail | kynos #998: no written use case an outside BodyCodec cannot serve |
| `cbor4ii` | 2 | pending | not measured: decode cycles at json-small, encode cycles at json-small, decode heap blocks at json-small, decode heap bytes at json-small, encode heap blocks at json-small, encode heap bytes at json-small, encoded size at json-small, encoded size gzip at json-small, encoded size zstd at json-small, decode cycles at echo-post, encode cycles at echo-post, decode heap blocks at echo-post, decode heap bytes at echo-post, encode heap blocks at echo-post, encode heap bytes at echo-post, encoded size at echo-post, encoded size gzip at echo-post, encoded size zstd at echo-post, decode cycles at json-large, encode cycles at json-large, decode heap blocks at json-large, decode heap bytes at json-large, encode heap blocks at json-large, encode heap bytes at json-large, encoded size at json-large, encoded size gzip at json-large, encoded size zstd at json-large |
| `cbor4ii` | 3 | pending | not measured: conformance at portable, conformance at native, no callgrind cells, no alloc cells, no hw cells, no time cells, no e2e-count routes, no e2e-time routes, fuzzing decode_value, fuzzing decode_struct, fuzzing roundtrip, Miri |
| `cbor4ii` | 4 | pending | not run to its end: fuzzing cbor4ii portable decode_value, fuzzing cbor4ii portable decode_struct, fuzzing cbor4ii portable roundtrip; no footprint |
| `cbor4ii` | 5 | pending | no footprint |
| `cbor2` | 1 | fail | kynos #998: no written use case an outside BodyCodec cannot serve |
| `cbor2` | 2 | pending | not measured: decode cycles at json-small, encode cycles at json-small, decode heap blocks at json-small, decode heap bytes at json-small, encode heap blocks at json-small, encode heap bytes at json-small, encoded size at json-small, encoded size gzip at json-small, encoded size zstd at json-small, decode cycles at echo-post, encode cycles at echo-post, decode heap blocks at echo-post, decode heap bytes at echo-post, encode heap blocks at echo-post, encode heap bytes at echo-post, encoded size at echo-post, encoded size gzip at echo-post, encoded size zstd at echo-post, decode cycles at json-large, encode cycles at json-large, decode heap blocks at json-large, decode heap bytes at json-large, encode heap blocks at json-large, encode heap bytes at json-large, encoded size at json-large, encoded size gzip at json-large, encoded size zstd at json-large |
| `cbor2` | 3 | pending | not measured: conformance at portable, conformance at native, no callgrind cells, no alloc cells, no hw cells, no time cells, no e2e-count routes, no e2e-time routes, fuzzing decode_value, fuzzing decode_struct, fuzzing roundtrip, Miri |
| `cbor2` | 4 | pending | not run to its end: fuzzing cbor2 portable decode_value, fuzzing cbor2 portable decode_struct, fuzzing cbor2 portable roundtrip; no footprint |
| `cbor2` | 5 | pending | no footprint |
| `c2pa_cbor` | 1 | fail | kynos #998: no written use case an outside BodyCodec cannot serve |
| `c2pa_cbor` | 2 | pending | not measured: decode cycles at json-small, encode cycles at json-small, decode heap blocks at json-small, decode heap bytes at json-small, encode heap blocks at json-small, encode heap bytes at json-small, encoded size at json-small, encoded size gzip at json-small, encoded size zstd at json-small, decode cycles at echo-post, encode cycles at echo-post, decode heap blocks at echo-post, decode heap bytes at echo-post, encode heap blocks at echo-post, encode heap bytes at echo-post, encoded size at echo-post, encoded size gzip at echo-post, encoded size zstd at echo-post, decode cycles at json-large, encode cycles at json-large, decode heap blocks at json-large, decode heap bytes at json-large, encode heap blocks at json-large, encode heap bytes at json-large, encoded size at json-large, encoded size gzip at json-large, encoded size zstd at json-large |
| `c2pa_cbor` | 3 | pending | not measured: conformance at portable, conformance at native, no callgrind cells, no alloc cells, no hw cells, no time cells, no e2e-count routes, no e2e-time routes, fuzzing decode_value, fuzzing decode_struct, fuzzing roundtrip, Miri |
| `c2pa_cbor` | 4 | pending | not run to its end: fuzzing c2pa_cbor portable decode_value, fuzzing c2pa_cbor portable decode_struct, fuzzing c2pa_cbor portable roundtrip; no footprint |
| `c2pa_cbor` | 5 | pending | no footprint |

