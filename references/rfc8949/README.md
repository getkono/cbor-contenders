# RFC 8949

- `rfc8949.txt`: RFC 8949, *Concise Binary Object Representation (CBOR)*,
  unmodified, from <https://www.rfc-editor.org/rfc/rfc8949.txt>, retrieved
  2026-11-11.
- `appendix_a.toml`: Table 6 (Appendix A) transcribed, with the type each
  row is decoded into and the value it must decode to. `conformance` checks
  that every row's diagnostic notation and encoding are the RFC's, in order.

The machine-readable vectors at `github.com/cbor/test-vectors`
(`appendix_a.json`) are not used: that repository carries no licence, its
vectors are RFC 7049's, and they include `0xf818` (`simple(24)` in two
bytes), which RFC 8949 section 3.3 makes ill-formed.

Licences in `LICENSE`; digests in `SHA256SUMS`, which `conformance` checks.
