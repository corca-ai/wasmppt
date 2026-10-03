# Documentation

Start with the path matching your task. All living guides are directly in `docs/`; subsystem pages
own detailed contracts, while source/configuration files own changing inventories and measurements.

## Use the library

1. Read [getting started](getting-started.md) to choose binding injection, semantic deck generation,
   or presentation viewing and install a pinned source revision.
2. Follow [browser integration](browser.md) for Worker startup, generation, rendering, HTML export,
   and cleanup, or [host adapters](hosts.md) for native and Cloudflare integration.
3. Read the relevant [subsystem contract](#subsystem-contracts) before relying on feature semantics.
   Check [advanced-content support](advanced-content.md) and [release readiness](release.md) for
   fidelity and stability limits.

The [playground](playground.md) is a local browser demonstration using bundled templates.

## Contribute

1. Read [architecture](architecture.md) for invariants and ownership boundaries.
2. Follow [development](develop.md) to bootstrap, enable hooks, find entry points, and verify work.
3. Read the owning subsystem page below before editing its code or public contract.
4. Use [quality gates](quality.md) to choose additional evidence and
   [documentation rules](metadoc.md) to update the same change's docs and links.

Implementation work and acceptance criteria live in
[GitHub Issues](https://github.com/corca-ai/wasmppt/issues).

## Subsystem contracts

Read each row from left to right when learning that pipeline.

| Area | Reading order and purpose |
| --- | --- |
| Package core | [OPC/ZIP](opc.md) for bounded I/O and raw copies → [OOXML graph](ooxml.md) for source ranges, relationships, and conformance |
| Authored templates | [Bindings](bindings.md) for authoring and plan identity → [injection](injection.md) for generation operations → [live editing](live-editing.md) for revisions and invalidation |
| Semantic decks | [Deck contracts](deck-engine.md) for source and wire types → [Starter compiler](deck-template.md) for POTX profiles → [layout](deck-layout.md) for measurement/pagination → [composition](deck-compose.md) for editable output |
| Rendering | [Slide resolution](rendering.md) for inheritance and WPDL → [Canvas](canvas.md) for interactive drawing → [DOM/SVG](dom-svg.md) for accessible standalone HTML and browser PDF input |
| Feature limits | [Advanced content](advanced-content.md) for table/chart semantics, fallbacks, and the capability matrix |
| Integration | [Host adapters](hosts.md) for ownership and protocols → [playground](playground.md) for a complete example and static deployment |

## Verification and publication

- [Quality gates](quality.md): local, PR, scheduled, and release responsibilities.
- [Compatibility gates](compatibility.md): security, preservation, pixels, and Office consumers.
- [Corpus](corpus.md): provenance, fixture registration, regeneration, and scorecards.
- [Deck gate](deck-gates.md): semantic fixture regeneration and exact cross-host comparisons.
- [Performance](performance.md): reproducible workloads, raw measurements, budgets, and claims.
- [Release readiness](release.md): stability, provenance, support, and publication checklist.
- [Documentation rules](metadoc.md): flat structure, durable prose, contextual links, and linting.

## Standards

- [ECMA-376](https://ecma-international.org/publications-and-standards/standards/ecma-376/): normative OOXML and packaging vocabulary.
- [PresentationML structure](https://learn.microsoft.com/en-us/office/open-xml/presentation/structure-of-a-presentationml-document): Microsoft's introduction to presentation parts.
- [Workers WebAssembly](https://developers.cloudflare.com/workers/runtime-apis/webassembly/): runtime capabilities to verify before changing a Cloudflare build profile.
- [Workers limits](https://developers.cloudflare.com/workers/platform/limits/): platform ceilings to check when choosing adapter budgets.
