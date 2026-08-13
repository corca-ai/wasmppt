# Release Readiness

This checklist defines the evidence required before publishing a stable `wasmppt` crate, npm
package, or host artifact. Pre-alpha workspace builds are not releases and make no semver promise.

## Versioning and public surface

- [ ] Choose one coordinated version for Rust crates, npm packages, generated Wasm glue, and
  protocol documentation.
- [ ] Classify every package-root TypeScript export and Rust facade item as stable, experimental,
  or private.
- [ ] Document semver treatment for non-exhaustive Rust errors, WPPD/WPDL version negotiation,
  error-envelope fields, and optional Wasm modules.
- [ ] Generate rustdoc and TypeScript declarations with no broken links or undocumented
  lifecycle-sensitive entry points.
- [ ] Publish a migration note for every breaking change since the previous release.

## Artifacts and provenance

- [ ] Build scalar, metafile, and shaper Wasm artifacts once from the tagged revision and reuse
  those exact bytes in host, performance, and publication jobs.
- [ ] Record checksums, compiler versions, `wasm-bindgen` version, licenses, source revision, and
  software-bill-of-materials metadata.
- [ ] Verify npm package contents and Rust crate contents from clean archives rather than the
  working tree.
- [ ] Sign the release and retain immutable CI evidence for every published artifact.

## Compatibility and performance evidence

- [ ] Pass native, real-browser, and workerd host parity with identical generated PPTX bytes.
- [ ] Pass Microsoft Open XML validation, LibreOffice/PowerPoint/Keynote compatibility policy,
  corpus scorecards, fuzz/limit gates, and visual tolerance reports.
- [ ] Publish raw benchmark samples and margins for all declared fixture sizes and hosts.
- [ ] Confirm peak memory, Wasm size, latency, preservation, and invalidation contracts on the
  release build rather than a local substitute.

## Support and operations

- [ ] State supported Rust, Node.js, browser, Cloudflare compatibility-date, and PowerPoint
  producer/consumer ranges.
- [ ] Publish security reporting, vulnerability response, deprecation window, and end-of-support
  policies.
- [ ] Document ownership, cancellation, retry, error-code, resource-limit, and explicit-release
  behavior for each host.
- [ ] Provide installation, generation, rendering, R2, troubleshooting, and rollback examples.
- [ ] Assign release owners and complete a post-publication install and smoke test from public
  registries.

See the [compatibility gates](compatibility.md), [performance contract](performance.md), and
[runtime host adapters](hosts.md) for the evidence behind this checklist.
