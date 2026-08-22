import { cp, mkdir, rm } from 'node:fs/promises'
import { resolve } from 'node:path'

import { copyEsmClosure } from './esm-closure.mjs'

const root = resolve(import.meta.dirname, '..')
const output = resolve(root, 'target/pages')
await rm(output, { recursive: true, force: true })
await mkdir(resolve(output, 'fixtures'), { recursive: true })
for (const file of ['index.html', 'style.css']) {
  await cp(resolve(root, 'examples/browser-dogfood', file), resolve(output, file))
}
const modules = await copyEsmClosure({
  entries: [
    resolve(output, 'app.js'),
    resolve(output, 'worker.js'),
    resolve(output, 'lib/shaper.js'),
    resolve(output, 'wasm/shaper/wasmppt_shaper_wasm.js'),
    resolve(output, 'wasm/shaper/wasmppt_shaper_wasm_bg.wasm'),
    resolve(output, 'wasm/wasmppt_wasm_bg.wasm'),
    resolve(output, 'wasm/metafile/wasmppt_metafile_wasm_bg.wasm'),
  ],
  mounts: [
    { sourceRoot: resolve(root, 'examples/browser-dogfood'), outputRoot: output },
    { sourceRoot: resolve(root, 'packages/wasmppt/dist'), outputRoot: resolve(output, 'lib') },
    {
      sourceRoot: resolve(root, 'packages/wasmppt-worker/src/generated'),
      outputRoot: resolve(output, 'wasm'),
    },
  ],
  outputRoot: output,
})
await cp(resolve(root, 'fixtures/dogfood/report.potx'), resolve(output, 'fixtures/report.potx'))
await cp(resolve(root, 'fixtures/dogfood/garden.potx'), resolve(output, 'fixtures/garden.potx'))
console.log(`GitHub Pages artifact: ${output} (${modules.length} modules and explicit assets)`)
