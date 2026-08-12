import { cp, mkdir, rm } from 'node:fs/promises'
import { resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const output = resolve(root, 'target/pages')
await rm(output, { recursive: true, force: true })
await mkdir(resolve(output, 'lib'), { recursive: true })
await mkdir(resolve(output, 'wasm'), { recursive: true })
await mkdir(resolve(output, 'fixtures'), { recursive: true })
for (const file of ['index.html', 'style.css', 'app.js', 'worker.js']) {
  await cp(resolve(root, 'examples/browser-dogfood', file), resolve(output, file))
}
for (const file of ['worker-client.js', 'worker-runtime.js', 'protocol.js', 'injection.js', 'canvas.js']) {
  await cp(resolve(root, 'packages/wasmppt/dist', file), resolve(output, 'lib', file))
}
for (const file of ['wasmppt_wasm.js', 'wasmppt_wasm_bg.wasm']) {
  await cp(
    resolve(root, 'packages/wasmppt-worker/src/generated', file),
    resolve(output, 'wasm', file),
  )
}
await cp(resolve(root, 'fixtures/dogfood/report.potx'), resolve(output, 'fixtures/report.potx'))
await cp(resolve(root, 'fixtures/host-adapters/minimal.potx'), resolve(output, 'fixtures/minimal.potx'))
console.log(`GitHub Pages artifact: ${output}`)
