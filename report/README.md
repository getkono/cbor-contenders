# Results

Nothing has been measured yet: `results/` holds no measured file. Run `mise run all` on a quiet x86-64 Linux host (`CC_TRUST=canonical mise run all` once it is quieted), commit `results/`, and run `cargo xtask report`.

<a id="verdict"></a>

## Verdict (kynos #989 section 7)

CBOR is admitted in-tree only if every clause holds. Clause 1 was settled by kynos #998 before anything here ran; the rest are computed from the results.

| Candidate | Admitted | 1 use case | 2 frontier | 3 conformance | 4 soundness | 5 licence |
| --- | --- | --- | --- | --- | --- | --- |
| `ciborium` | no | fail | pending | pending | pending | pending |
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
| `ciborium` | 2 | pending | not measured: decode cycles at json-small, encode cycles at json-small, decode heap bytes at json-small, encode heap bytes at json-small, encoded size at json-small, decode cycles at echo-post, encode cycles at echo-post, decode heap bytes at echo-post, encode heap bytes at echo-post, encoded size at echo-post, decode cycles at json-large, encode cycles at json-large, decode heap bytes at json-large, encode heap bytes at json-large, encoded size at json-large |
| `ciborium` | 3 | pending | not measured: conformance at portable, conformance at native, fuzzing, Miri |
| `ciborium` | 4 | pending | no footprint |
| `ciborium` | 5 | pending | no footprint |
| `minicbor-serde` | 1 | fail | kynos #998: no written use case an outside BodyCodec cannot serve |
| `minicbor-serde` | 2 | pending | not measured: decode cycles at json-small, encode cycles at json-small, decode heap bytes at json-small, encode heap bytes at json-small, encoded size at json-small, decode cycles at echo-post, encode cycles at echo-post, decode heap bytes at echo-post, encode heap bytes at echo-post, encoded size at echo-post, decode cycles at json-large, encode cycles at json-large, decode heap bytes at json-large, encode heap bytes at json-large, encoded size at json-large |
| `minicbor-serde` | 3 | pending | not measured: conformance at portable, conformance at native, fuzzing, Miri |
| `minicbor-serde` | 4 | pending | no footprint |
| `minicbor-serde` | 5 | pending | no footprint |
| `minicbor` | 1 | fail | kynos #998: no written use case an outside BodyCodec cannot serve |
| `minicbor` | 2 | pending | not measured: decode cycles at json-small, encode cycles at json-small, decode heap bytes at json-small, encode heap bytes at json-small, encoded size at json-small, decode cycles at echo-post, encode cycles at echo-post, decode heap bytes at echo-post, encode heap bytes at echo-post, encoded size at echo-post, decode cycles at json-large, encode cycles at json-large, decode heap bytes at json-large, encode heap bytes at json-large, encoded size at json-large |
| `minicbor` | 3 | fail | its wire form (a struct as an array) has no faithful JSON Schema (section 5) |
| `minicbor` | 4 | pending | no footprint |
| `minicbor` | 5 | pending | no footprint |
| `cbor4ii` | 1 | fail | kynos #998: no written use case an outside BodyCodec cannot serve |
| `cbor4ii` | 2 | pending | not measured: decode cycles at json-small, encode cycles at json-small, decode heap bytes at json-small, encode heap bytes at json-small, encoded size at json-small, decode cycles at echo-post, encode cycles at echo-post, decode heap bytes at echo-post, encode heap bytes at echo-post, encoded size at echo-post, decode cycles at json-large, encode cycles at json-large, decode heap bytes at json-large, encode heap bytes at json-large, encoded size at json-large |
| `cbor4ii` | 3 | pending | not measured: conformance at portable, conformance at native, fuzzing, Miri |
| `cbor4ii` | 4 | pending | no footprint |
| `cbor4ii` | 5 | pending | no footprint |
| `cbor2` | 1 | fail | kynos #998: no written use case an outside BodyCodec cannot serve |
| `cbor2` | 2 | pending | not measured: decode cycles at json-small, encode cycles at json-small, decode heap bytes at json-small, encode heap bytes at json-small, encoded size at json-small, decode cycles at echo-post, encode cycles at echo-post, decode heap bytes at echo-post, encode heap bytes at echo-post, encoded size at echo-post, decode cycles at json-large, encode cycles at json-large, decode heap bytes at json-large, encode heap bytes at json-large, encoded size at json-large |
| `cbor2` | 3 | pending | not measured: conformance at portable, conformance at native, fuzzing, Miri |
| `cbor2` | 4 | pending | no footprint |
| `cbor2` | 5 | pending | no footprint |
| `c2pa_cbor` | 1 | fail | kynos #998: no written use case an outside BodyCodec cannot serve |
| `c2pa_cbor` | 2 | pending | not measured: decode cycles at json-small, encode cycles at json-small, decode heap bytes at json-small, encode heap bytes at json-small, encoded size at json-small, decode cycles at echo-post, encode cycles at echo-post, decode heap bytes at echo-post, encode heap bytes at echo-post, encoded size at echo-post, decode cycles at json-large, encode cycles at json-large, decode heap bytes at json-large, encode heap bytes at json-large, encoded size at json-large |
| `c2pa_cbor` | 3 | pending | not measured: conformance at portable, conformance at native, fuzzing, Miri |
| `c2pa_cbor` | 4 | pending | no footprint |
| `c2pa_cbor` | 5 | pending | no footprint |

