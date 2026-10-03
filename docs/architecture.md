# System architecture

`wasmppt` is a loss-aware Rust engine for reading, transforming, writing, and rendering
PowerPoint Open XML packages. Native, browser, and Cloudflare adapters share the deterministic
core. This page defines boundaries; subsystem pages define APIs and supported behavior.
For runnable integration steps, start with [getting started](getting-started.md).

## Invariants

- Preserve unknown parts, relationships, and XML outside an explicitly edited or removed scope.
- Keep host APIs outside the core. Parsing, planning, and composition must work without a DOM,
  JavaScript runtime, filesystem, or Cloudflare binding.
- Compile repeated work once. Reuse cached results only when all relevant inputs and dependencies
  match; missing proof selects a conservative path.
- Bound parsing, planning, retained state, and output chunks. Streaming output does not imply that
  input bytes or the dirty-entry working set occupy no memory.
- Keep preview and export on the same immutable revision.
- Gate performance claims on correctness and reproducible evidence.

## Pipelines

```text
Authored POTX/POTM + bindings         Starter POTX + semantic DeckSpec
             |                                   |
      TemplateCompiler                    ThemeTemplateCompiler
             |                                   |
      PreparedTemplate                 DeckTemplatePlan + DeckPlanner
             |                                   |
      injection / LiveSession                  DeckPlan
             |                                   |
      PreparedOverlay                    deck composition
             |                                   |
             +----------- package view ----------+
                                |
                 +--------------+--------------+
                 |                             |
             ZIP export                 PresentationDocument
                                               |
                                      lazy slide resolution
                                               |
                                          WPDL scene
                                               |
                                  Canvas or offline DOM/SVG
```

An existing PPTX enters at `PresentationDocument`. Generation does not require a renderer.
The two template pipelines are separate contracts, described below.

## Package and XML ownership

PowerPoint files are ZIP packages containing a graph of parts. The
[OPC substrate](opc.md) owns bounded indexing, random-access `ReadAt` sources, forward-only
`OutputSink` output, deterministic headers, and verbatim compressed copying. It rejects unsupported
ZIP forms before semantic mutation. Changed entries require bounded input/compression buffers;
unchanged entries stream from the indexed source.

The [OOXML graph](ooxml.md) owns content types, relationships, conformance, reachability, and
stable diagnostics. XML tokens retain namespace-resolved names and exact byte ranges. Typed
PresentationML views interpret supported features without reconstructing all unknown markup.
Cycles are legal graph inputs and traversal is visit-bounded.

An immutable overlay exposes new, replaced, removed, and unchanged parts through one
`PackagePartSource`. Layout can read it directly, and export drains that same view without
serializing and reopening a preview ZIP. Preservation is scoped: digital signatures and active
content follow the selected conversion policy rather than an unconditional byte-copy promise.

## Authored-template generation

[Bindings](bindings.md) identify authored shapes through metadata, a manifest, or split-run visible
tokens. `TemplateCompiler` produces a source-verified, versioned `TemplatePlan`;
`PreparedTemplate` caches the parts and ranges needed for repeated generation.

[Injection](injection.md) applies text, image, table, chart/workbook, shape, notes, and slide
operations. Warm generation recompresses dirty/new entries and raw-copies unrelated entries.
Macro stripping or rejection is explicit; macro-preserving PPTM output is not exposed.

[Live sessions](live-editing.md) retain complete generation data and an immutable overlay at a
monotonic revision. Deltas commit only after validation. Dependency fingerprints permit scene and
resource reuse; topology changes or incomplete dependency proof take the broader path. Export
reads the accepted overlay rather than repeating injection.

## Semantic deck generation

A host authoring adapter supplies [DeckSpec](deck-engine.md), source identities, factual media/text
relations, and authorized resource bytes. Markdown parsing, network authorization, and product UI
stay in the host. Media dimensions from the host are hints checked against bounded resource bytes.

The [semantic SDK](semantic-sdk.md) accepts typed string keys independently of current source
spans. It owns wire encoding, content-addressed asset registrations, exact font inputs, structured
template inspection, and immutable accepted snapshots. A full template/spec/resource/font tuple
replaces the deck atomically; retained snapshots remain usable during subsequent edits. Node and
browser adapters share this API without depending on any consumer project.

The [Starter compiler](deck-template.md) discovers explicit layout identities, resolves exact page
geometry, text styles, theme assets, and safe content envelopes into `DeckTemplatePlan`.
The [planner](deck-layout.md) owns measurement, candidate geometry, readable-size floors,
contain/cover decisions, crop bounds, pagination, and continuation metadata. Exact font bytes use
HarfRust; deterministic fallback metrics emit diagnostics.

Validators prove complete ordered source coverage, stable identities, compatible region ownership,
and valid geometry. The [composer](deck-compose.md) consumes the exact spec/template/plan tuple,
creates editable PresentationML and coordinated chart/workbook parts, and retains untouched
compressed template parts. It does not independently reflow or refit media.

## Rendering and fonts

[Lazy resolution](rendering.md) follows a requested slide's theme/master/layout branch and lowers
resolved geometry, text, resources, diagnostics, and semantics to the versioned binary WPDL format.
The package graph defines invalidation; resources remain lazy package-part references.

[Canvas](canvas.md) owns interactive drawing, viewport scheduling, resource caches, and browser font
registration. [DOM/SVG](dom-svg.md) projects the same scene and positioned text plan into selectable,
accessible offline HTML with exact page geometry and closed resources. It also provides browser
PDF print input; the library does not contain a native PDF renderer.

Font substitution is observable. Browser measurement and optional exact font-byte shaping feed a
shared text layout plan. The optional `wasmppt-shaper-wasm` artifact uses HarfRust; the separate
`wasmppt-metafile-wasm` artifact converts supported EMF/WMF content to SVG. Browser loading of these
modules is demand-driven, while semantic core planning can use `wasmppt-shaper` directly.
Generation-only HTTP requests do not instantiate the optional browser modules.

Unsupported drawing features retain source content and explicit diagnostics or a bounded recorded
fallback. [Advanced-content policy](advanced-content.md) and the
[capability matrix](../capabilities/presentationml.json) separate preservation from rendering and
editing support. Exact pixels without the intended fonts, native SmartArt layout, general 3D,
and animation playback are outside the current renderer.

## Hosts and public boundaries

The [host adapters](hosts.md) own transfer, I/O, cancellation, errors, and memory accounting:

- Native file adapters implement the core I/O capabilities and provide reference execution for
  profiling, fuzzing, and compatibility inspection.
- Browser module Workers own handles and package/session state. The semantic SDK copies caller
  inputs, manages handles, and exposes disposable snapshots. Its Canvas view owns lazy resources,
  caches and complete-frame publication. The lower-level adapter transfers input ownership and
  requires explicit handle/revision management.
- Cloudflare HTTP endpoints stream generation from request or R2 template bytes. Only immutable
  prepared templates enter the isolate cache; mutable live sessions stay request-local. Browser
  Canvas and DOM APIs are not part of this HTTP surface.

Crate ownership and package exports are listed in the [development guide](develop.md). Rust facades,
package-root TypeScript exports, versioned binary envelopes, and structured errors remain pre-alpha;
[release readiness](release.md) defines the future stability requirements. The semantic SDK is the public authoring facade; binary protocols remain lower-level contracts.

## Determinism, security, and evidence

Given identical engine bytes, input, and options, deterministic generation fixes entry order,
timestamps, ID allocation, and serialization policy. Cache identities include schema, producer,
source, and policy inputs. A mismatch is a cache miss. [Host parity](hosts.md) and the
[deck gate](deck-gates.md) compare actual output bytes across native, browser, and workerd.

Every package is untrusted. Limits cover ZIP/XML structure, inflation, traversal, payloads,
resources, planning work, and retained overlays. Active content is never executed. External
resource loading and authorization belong to hosts; offline output rejects unresolved or unsafe
required resources rather than emitting a partial document.

The [quality gates](quality.md) assign checks to local, PR, scheduled, and release tiers.
[Compatibility gates](compatibility.md) distinguish structural validity, preservation, browser
pixels, and controlled Office evidence. The [corpus](corpus.md) records provenance and redistribution
permission. The [performance contract](performance.md) defines reproducible workloads, raw evidence,
and latency, memory, binary-size, and raw-copy budgets; current measurements live with artifacts.

Software rasterization for hosts without Canvas, macro-preserving output, and a full editor UI are
not implemented. Execution plans and acceptance work belong in
[GitHub Issues](https://github.com/corca-ai/wasmppt/issues), not a second roadmap here.
