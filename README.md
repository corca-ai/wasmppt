# wasmppt

`wasmppt` is a high-performance Rust/WebAssembly library for reading, transforming,
writing, and rendering PowerPoint Open XML files in browsers, Cloudflare Workers,
and native runtimes.

The first performance target is repeated generation of `.pptx` presentations from
compiled `.potm`/`.potx` templates. The first rendering target is a lazy slide scene
pipeline with Canvas 2D and DOM/SVG backends.

The project is pre-alpha. Generation API v2 (with v1 decoding) and the rendering pipeline are implemented,
but no stable semver API has been released yet.

Try the browser-only [wasmppt playground](https://corca-ai.github.io/wasmppt/). It compiles
POTX/POTM files and generates PPTX downloads locally; uploaded files never leave the page.

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
