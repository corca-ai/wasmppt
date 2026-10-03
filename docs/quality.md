# Quality Gates

Quality is layered so ordinary edits get fast feedback while expensive compatibility and runtime
evidence remains authoritative before release. Workflow actions are pinned to full commits and managed through
[Dependabot configuration](../.github/dependabot.yml); tool-version changes are reviewed in the
[CI workflow](../.github/workflows/ci.yml) and [scheduled workflow](../.github/workflows/rust-deep-quality.yml).

## Gate ownership

| Tier | Entry point | Required evidence | Owner |
| --- | --- | --- | --- |
| Commit | `npm run precommit` | Formatting, lint, types, contracts, docs, Rust library and host-free package tests | Contributor |
| Push | `npm run prepush` | Cargo check/Clippy/tests/doctests/Wasm, package and workerd tests, dependency policy | Contributor |
| Pull request | `CI` | Portable correctness, MSRV, coverage ratchet, hosts, compatibility, security, visual and performance contracts | Maintainers |
| Scheduled | `Rust deep quality`, `Full compatibility corpus` | Bounded fuzzing, Miri, three-OS smoke, full corpus | Maintainers |
| Release | `Office ground truth` plus the release checklist | PowerPoint/LibreOffice/Keynote evidence and artifact review | Release manager |

The required portable merge checks are `Quality / repository`, `Rust / native correctness`,
`Rust / MSRV`, `Packages / TypeScript and tests`, and `Security / dependency policy`. Host,
compatibility, coverage, and performance jobs remain required while the repository has available
GitHub-hosted capacity. Self-hosted Office consumers are release evidence and are not portable
merge blockers.

## Pinned Rust quality tools

- cargo-nextest runs native unit and integration tests in pull requests. Doctests remain a
  separate `cargo test --doc` step because nextest does not execute them.
- cargo-llvm-cov writes JSON and LCOV evidence; [the coverage script](../scripts/coverage-core.sh)
  selects the core crates to measure.
- cargo-machete rejects unused dependencies. An ignore is allowed only with an adjacent
  manifest comment or issue explaining the false positive and its removal condition.
- cargo-deny exclusively owns Rust advisories, license allowlists, duplicate versions, and
  dependency sources. Do not add a second advisory scanner with a conflicting policy.
- cargo-fuzz runs the checked-in targets with bounded execution time; the scheduled workflow
  defines the time budget and artifact retention.

Install the workflow-pinned local versions when using the corresponding optional commands. Hooks never
install tools or dependencies.

## Coverage ratchet

Run `npm run coverage:core` after installing cargo-llvm-cov and the `llvm-tools-preview` component.
The checked-in [coverage baseline](../quality/coverage-baseline.json) records line, function, and
region percentages rather than imposing an arbitrary aspirational threshold. Pull requests may not
lower metrics beyond the tolerance in [the coverage checker](../scripts/check-coverage.mjs). A baseline update must include the generated
summary artifact and explain why a measured increase should become the new floor; decreases require
an explicit maintainer-approved exception.

## Scheduled deep checks

The Miri subset is intentionally limited to XML token integration tests and template payload unit
tests. These modules are deterministic, host-agnostic, and do not require filesystem, browser, or
Wasm behavior. The scheduled workflow pins its nightly toolchain and enables strict provenance. Add a test to
this subset only after it succeeds under the pinned nightly; record a concrete incompatibility in a
GitHub issue rather than silently skipping it.

The native smoke matrix runs the file adapter and CLI validator on Linux, macOS, and Windows. The
fuzz job invokes `scripts/run-fuzz-ci.sh`, which enumerates every target explicitly so a new target
must update both the script and its contract test.

Mutation testing is not yet a standing gate. The XML/package parsers already have property tests,
fuzz surfaces, limit tests, and a coverage ratchet; a mutation pilot would currently add high
runtime for overlapping signal. Reconsider a narrowly scoped parser-policy pilot when a production
escape or repeated review ambiguity identifies a mutation class that these gates miss.

## Quarantine and release policy

A portable required check may be quarantined only for a linked incident with an owner, expiry date,
and compensating command. Never replace a failing check with unconditional success. Scheduled and
self-hosted jobs may be temporarily disabled only in their workflow condition and must retain the
incident link. Restore quarantined checks before a release.

Crates remain version `0.0.0` and `publish = false`, so cargo-semver-checks has no truthful registry
or tag baseline today. Before the first publishable release, select an explicit signed Git tag,
enable pinned cargo-semver-checks against it, and make the result release-blocking. The same release
workflow must produce checksummed archives, a CycloneDX or SPDX SBOM, and GitHub artifact
attestations/provenance; these are release outputs, not ordinary pull-request compilation.

## Related documents

- [Development guide](develop.md) lists exact local commands and hook behavior.
- [Compatibility gates](compatibility.md) owns corpus, visual, and desktop-consumer policy.
- [Performance contract](performance.md) owns latency, memory, and binary-size budgets.
- [Release readiness](release.md) owns publication and support-policy decisions.
