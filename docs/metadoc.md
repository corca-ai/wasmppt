# Documentation Guide

This document defines how to write and maintain `wasmppt` documentation.

## Goal

Keep documentation easy to scan, easy to navigate, and trustworthy for users,
contributors, and coding agents.

## Principles

- Treat documentation as part of the tested product.
- Keep intent, invariants, and support boundaries explicit.
- Prefer small focused documents over long documents that mix unrelated lifecycles.
- Describe current behavior separately from proposed or planned behavior.
- Link to authoritative standards or platform documentation for unstable external facts.
- Do not version-control generated API documentation or benchmark output unless it is
  deliberate, revision-bound release evidence.

## Structure

- Keep `README.md` as the short user-facing entry point: mission, current status,
  and links into `docs/`; installation and examples belong in the guides.
- Keep `docs/index.md` as the canonical documentation map. Every living document must
  be reachable from it.
- Keep `AGENTS.md` concise. It routes contributors and agents to authoritative project
  documents rather than duplicating them.
- Keep `CLAUDE.md` as a symlink to `AGENTS.md` so both entry points stay identical.
- Keep living documents directly under `docs/` so the documentation graph remains flat.
- Keep generated fixtures, raw measurements, and immutable evidence in their existing artifact
  locations outside `docs/`; link them from the owning guide instead of making a second wiki tree.
- Keep implementation plans and task status in GitHub Issues. Keep durable decisions,
  contracts, and architecture in documentation.

## Required document content

Architecture and API documents should distinguish:

- implemented behavior;
- accepted design that is not implemented yet;
- non-goals and unsupported behavior;
- invariants that tests must enforce;
- compatibility, security, and performance consequences.

Use stable project terms from [System architecture](architecture.md). Update that
document in the same change when a term or boundary changes.

## Links

- Prefer relative Markdown links for repository documents.
- Link a new living document from [the documentation index](index.md) and from at least
  one related document when such a relationship exists.
- Prefer links where a concept is introduced. A related-document list must explain why each link
  matters, for example `- [Canvas](canvas.md): image, font, and drawing ownership.`
- Give library users and contributors explicit reading paths in the index. Keep installation,
  lifecycle examples, subsystem contracts, and contribution steps easy to distinguish.
- Remove or replace stale links in the same change that moves or deletes a document.
- Avoid empty placeholder pages. Create a GitHub issue until there is substantive content.

## Linting

Run these commands from the repository root before submitting a documentation change:

```sh
npm run lint:markdown
npm run check:contracts
awiki lint -root docs
```

Markdownlint validates Markdown structure and `awiki` validates the flat graph of living
documents. The `awiki` scan is intentionally non-recursive, so living pages must stay directly
under `docs/`. The contract synchronization check also
guards WPDL version claims in rendering documents against the Rust encoder, TypeScript decoder,
and capability matrix. All three commands must exit successfully.

## Writing rules

- Use concise headings and direct language.
- Prefer explicit requirements such as MUST, SHOULD, and MAY when documenting contracts.
- Define an acronym or specialist term on first use.
- Keep code examples aligned with the current public API; mark sketches as conceptual. Runnable
  TypeScript integration blocks are checked by `npm run check:doc-examples` against package exports.
- Do not duplicate test/fixture totals, current timings, line numbers, or line counts. Link the
  executable inventory, configuration, or revision-bound result instead. Protocol versions,
  units, and normative bounds may be stated when necessary; name their source and keep checks
  aligned with that contract.
- Keep one authoritative explanation per topic. Summarize and link rather than repeating feature
  histories across pages. Split a page when it mixes distinct workflows, not merely to retain
  obsolete material. Keep implementation history in issues or minimal linked evidence.
- Remove obsolete architecture terminology immediately rather than keeping historical
  descriptions in living documentation.
