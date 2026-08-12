# Loss-Aware OOXML Graph

This document describes the implemented XML, Open Packaging Conventions (OPC), and
PresentationML inspection layer. It builds on the [OPC and ZIP substrate](opc.md).

## XML token model

`wasmppt-xml` parses UTF-8 XML into namespace-resolved tokens while retaining the original
byte buffer and an exact byte range for every token and attribute value. Namespace URIs
are interned. Typed consumers compare expanded names rather than source prefixes, so
equivalent documents using different prefixes behave identically.

The original bytes remain authoritative. Unknown attributes, elements,
`mc:AlternateContent`, extension lists, comments, and processing instructions require no
typed representation to survive an unrelated raw-copy rewrite. DTD declarations are
rejected, malformed nesting and undeclared prefixes have stable error codes, and
attribute entities are decoded without expanding named external entities.

`wasmppt-pml` currently provides typed views for presentation slide relationship IDs and
slide shape metadata, descriptions, and DrawingML text runs. Each text run points back to
its exact source range. The view does not build a schema-sized object tree and does not
discard tokens it does not understand.

## Package graph

`PackageGraph` indexes every non-plumbing part using compact `PartId` values and interns
part names, relationship IDs and types, content types, and observed namespace URIs. It
parses default and override content types, package relationships, per-part relationships,
relative targets, and external targets.

Graph construction emits stable `DiagnosticCode` values for missing or duplicate content
types, malformed relationship documents, duplicate IDs, unsafe or missing targets,
relationship cycles, orphan relationship parts, unreachable parts, and mixed
conformance. Diagnostics retain a `PartId` when a failure belongs to a concrete source
part.

Traversal is iterative and visit-bounded. Cycles are detected without recursive descent,
and `walk_from` returns a limit error before exceeding the caller's maximum. Reachability
starts at internal package relationships; opaque parts that are intentionally retained
but not reachable are reported rather than silently deleted.

## Conformance

The graph reports `Transitional`, `Strict`, `Mixed`, or `Unknown` from conformance-bearing
content-type, relationship, PresentationML, and DrawingML namespaces. Markup Compatibility
namespaces do not by themselves turn a Strict package into a mixed package.

Transitional and Strict packages share namespace-aware presentation, relationship, and targeted
template-edit paths. Strict no-op and bound-text edits retain Strict conformance and raw-copy
unknown compressed entries. This is bounded Strict support, not a promise that every Strict-only
feature can be authored. A mixed result is always accompanied by a machine-readable diagnostic.

## Preservation contract

Graph inspection inflates only semantic plumbing and the presentation main part needed
for conformance detection. It does not inflate arbitrary opaque parts. A no-op package
rewrite copies all compressed payloads verbatim, including Office extension markup and
unknown binary parts. Later mutation layers MUST rewrite only dirty XML parts and MUST
copy every other entry through this raw path.

## Verification

Tests cover namespace prefixes, source ranges, DTD rejection, typed PresentationML views,
Transitional and Strict detection and targeted editing, external and missing targets, duplicate relationship
IDs, cycles, bounded traversal, orphans, extension markup, and compressed-byte
preservation. Fuzz targets exercise both raw ZIP opening and graph construction:

```sh
cargo fuzz run --fuzz-dir crates/wasmppt-opc/fuzz open_package
cargo fuzz run --fuzz-dir crates/wasmppt-opc/fuzz package_graph
```

## Related documents

- See the [system architecture](architecture.md) for the full mutation and rendering
  design.
- See [template bindings and TemplatePlan](bindings.md) for the authoring contract
  built on these source ranges.
- See the [development guide](develop.md) for repository verification commands.
- Return to the [documentation index](index.md) for the project map.
