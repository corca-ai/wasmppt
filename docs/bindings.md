# Template Bindings and TemplatePlan

This document is the version 2 authoring and caching contract for compiled PowerPoint
templates.

## Authoring bindings

The preferred authoring method is shape metadata. In PowerPoint, select a text shape,
open the Alt Text pane, and set its Description to:

```text
wasmppt:text:customer_name
```

Binding IDs MUST be 1–128 ASCII letters, digits, `_`, `-`, or `.`. The shape's existing
text and run formatting form the style source. Shape metadata is stable when visible text
is edited and avoids exposing implementation tokens to presentation viewers.

A visible token is a convenience fallback:

```text
Hello, {{customer_name}}
```

The compiler concatenates DrawingML text runs before finding tokens, so the binding still
works when PowerPoint splits `{{customer_name}}` across several `<a:r>` elements. The plan
records the exact participating source ranges and decoded offsets. Visible tokens are
lower priority than shape metadata.

For centrally managed templates, add `wasmppt/bindings.xml`:

```xml
<bindings xmlns="urn:wasmppt:bindings:v1">
  <bind id="quarter_title" kind="text"
        part="/ppt/slides/slide1.xml" shapeName="Quarter title"/>
  <bind id="revenue" kind="text"
        part="/ppt/slides/slide2.xml" shapeId="17"/>
</bindings>
```

A manifest selector may use `shapeName`, `shapeId`, or both. If both are present, both
MUST match. Manifest bindings have the highest priority. Version 1 implements `text` and
`image`; other kinds produce an `UnsupportedKind` diagnostic.

Picture shapes may use `wasmppt:image:binding_id` in Alt Text Description or `kind="image"`
in the manifest. See [high-speed template injection](injection.md) for image data and crop
semantics.

## Diagnostics

Compilation emits stable binding diagnostic codes:

- `MissingTarget` — a manifest selector found no shape;
- `DuplicateId` — equally preferred targets use the same binding ID;
- `AmbiguousTarget` — a manifest selector matches several shapes;
- `UnsupportedKind` — the requested binding kind is not implemented;
- `InvalidManifest` — XML or required attributes are invalid; and
- `InvalidSlide` — a candidate slide cannot produce a typed view.

Plans with unresolved or ambiguous bindings set their completeness flag to false. They
remain inspectable for tooling but MUST NOT enter the fast injection path.

## Compilation and cache identity

`TemplateCompiler` scans package structure and slides once and emits an immutable
`TemplatePlan`. Repeated data payloads consume that plan; they do not rediscover bindings.
The versioned binary representation starts with a schema marker and has a bounded decoder.
Its structural signature is SHA-256 over deterministic plan bytes and is identical for
native and Wasm builds of the same engine.

Cache identity includes:

- the SHA-256 of every source template byte;
- plan and binding schema versions;
- engine version;
- macro policy;
- PowerPoint compatibility profile; and
- compression profile.

`reuse_decision` compares every field and every completeness proof. Any mismatch returns
`Recompile`; stale plans are never used with a warning.

## Generation API v2 preparation

The browser adapter exposes compilation as `WasmpptWorkerClient.prepare(template, options)`.
The input `ArrayBuffer` is transferred to the Worker. A successful result contains:

- an opaque prepared-template handle and its conservative resident byte weight;
- the versioned binary `TemplatePlan`, suitable for an application-owned plan store;
- discovered binding descriptors with kind, source part, authoring source, shape ID, and
  shape name where available; and
- stable compilation diagnostics for authoring tools.

`PrepareOptions` selects macro, compatibility, compression, and visible-token policies. Passing
the returned plan into a later `prepare` call skips binding discovery only after the Rust core
decodes the plan and verifies its source-template identity. Option tags and binary plan schema
are stable inputs to cache identity; JavaScript object property order is not.

WPPD v2 reuses those stable binding IDs for conditional/repeated shapes, rich text, safe
hyperlinks, basic solid-fill edits, image-fit policies, and notes. Applications do not need to
persist shape IDs or relationship part names. A chart frame Description such as
`wasmppt:chart:sales` also compiles to the related chart and workbook, so `charts.sales` is a
stable transactional update. The v1 decoder remains supported for migration;
v2 invalid combinations fail before any output entry is written.

## Storage boundary

The Rust core exposes only a generic binary plan-store capability. Native files,
databases, Cloudflare storage, and browser storage belong to adapters. The browser package
provides `IndexedDbTemplatePlanStore`, keyed by binary cache identity, without introducing
IndexedDB types into a core crate.

## Related documents

- See the [loss-aware OOXML graph](ooxml.md) for source ranges and typed shape views.
- See the [system architecture](architecture.md) for the compiled-template pipeline.
- Return to the [documentation index](index.md) for the project map.
