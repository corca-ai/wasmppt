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
  primary goals, and links into `docs/`.
- Keep `docs/index.md` as the canonical documentation map. Every living document must
  be reachable from it.
- Keep `AGENTS.md` concise. It routes contributors and agents to authoritative project
  documents rather than duplicating them.
- Keep `CLAUDE.md` as a symlink to `AGENTS.md` so both entry points stay identical.
- Keep living documents directly under `docs/` so the documentation graph remains flat.
- Use a subdirectory only for a cohesive collection with a different lifecycle or naming
  policy, such as immutable release evidence or generated compatibility fixtures. Give
  each such directory its own `README.md` describing those rules.
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
- Remove or replace stale links in the same change that moves or deletes a document.
- Avoid empty placeholder pages. Create a GitHub issue until there is substantive content.

## Linting

Run this command from the repository root before submitting a documentation change:

```sh
awiki lint -root docs
```

The scan is intentionally non-recursive. It validates the flat graph of living documents;
exceptional subdirectories govern their own contents through their local `README.md`.
The command must exit successfully.

## Writing rules

- Use concise headings and direct language.
- Prefer explicit requirements such as MUST, SHOULD, and MAY when documenting contracts.
- Define an acronym or specialist term on first use.
- Keep code examples aligned with the current public API; mark sketches as conceptual.
- Remove obsolete architecture terminology immediately rather than keeping historical
  descriptions in living documentation.
