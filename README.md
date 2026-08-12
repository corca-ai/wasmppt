# wasmppt

`wasmppt` is a high-performance Rust/WebAssembly library for reading, transforming,
writing, and rendering PowerPoint Open XML files in browsers, Cloudflare Workers,
and native runtimes.

The first performance target is repeated generation of `.pptx` presentations from
compiled `.potm`/`.potx` templates. The first rendering target is a lazy slide scene
pipeline with Canvas 2D and DOM/SVG backends.

The project is in its architecture and bootstrap phase. No stable API has been
released yet.

## Project documentation

- [Documentation index](docs/index.md)
- [System architecture](docs/architecture.md)
- [Contributor and agent guide](AGENTS.md)

Implementation work is tracked in [GitHub Issues](https://github.com/corca-ai/wasmppt/issues).

## Guiding goals

- Run the same deterministic Rust core across supported hosts.
- Preserve unsupported OOXML content instead of silently discarding it.
- Compile templates once and make repeated injection proportional to the changed parts.
- Stream large inputs and outputs with bounded memory.
- Publish reproducible compatibility and performance evidence before making speed claims.

## License

[MIT](LICENSE)
