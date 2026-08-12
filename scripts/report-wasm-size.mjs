import { appendFileSync, statSync } from 'node:fs'
import { resolve } from 'node:path'

const artifact = resolve(process.argv[2] ?? '')
const bytes = statSync(artifact).size
const result = { artifact, bytes }

console.log(JSON.stringify(result))

if (process.env.GITHUB_STEP_SUMMARY) {
  appendFileSync(
    process.env.GITHUB_STEP_SUMMARY,
    `## Wasm artifact size\n\n- \`${artifact}\`: ${bytes.toLocaleString('en-US')} bytes\n`,
  )
}
