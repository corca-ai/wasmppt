import { cp, mkdir, rm } from 'node:fs/promises'
import { resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const output = resolve(root, 'target/pages')
await rm(output, { recursive: true, force: true })
await mkdir(resolve(output, 'lib'), { recursive: true })
await mkdir(resolve(output, 'wasm'), { recursive: true })
await mkdir(resolve(output, 'wasm/metafile'), { recursive: true })
await mkdir(resolve(output, 'wasm/shaper'), { recursive: true })
await mkdir(resolve(output, 'fixtures'), { recursive: true })
for (const file of ['index.html', 'style.css', 'app.js', 'worker.js']) {
  await cp(resolve(root, 'examples/browser-dogfood', file), resolve(output, file))
}
for (const file of ['worker-client.js', 'worker-runtime.js', 'protocol.js', 'injection.js', 'canvas.js', 'shaper.js']) {
  await cp(resolve(root, 'packages/wasmppt/dist', file), resolve(output, 'lib', file))
}
for (const file of ['wasmppt_shaper_wasm.js', 'wasmppt_shaper_wasm_bg.wasm']) {
  await cp(
    resolve(root, 'packages/wasmppt-worker/src/generated/shaper', file),
    resolve(output, 'wasm/shaper', file),
  )
}
for (const file of ['wasmppt_wasm.js', 'wasmppt_wasm_bg.wasm']) {
  await cp(
    resolve(root, 'packages/wasmppt-worker/src/generated', file),
    resolve(output, 'wasm', file),
  )
}
for (const file of ['wasmppt_metafile_wasm.js', 'wasmppt_metafile_wasm_bg.wasm']) {
  await cp(
    resolve(root, 'packages/wasmppt-worker/src/generated/metafile', file),
    resolve(output, 'wasm/metafile', file),
  )
}
await cp(resolve(root, 'fixtures/dogfood/report.potx'), resolve(output, 'fixtures/report.potx'))
await cp(resolve(root, 'fixtures/dogfood/garden.potx'), resolve(output, 'fixtures/garden.potx'))
console.log(`GitHub Pages artifact: ${output}`)
