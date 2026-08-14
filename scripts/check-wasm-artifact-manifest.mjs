import { createHash } from 'node:crypto'
import { readFile, writeFile } from 'node:fs/promises'
import { spawnSync } from 'node:child_process'
import { dirname, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const manifestPath = resolve(
  root,
  'packages/wasmppt-worker/src/generated/artifact-manifest.json',
)
const artifactPaths = [
  'packages/wasmppt-worker/src/generated/wasmppt_wasm_bg.wasm',
  'packages/wasmppt-worker/src/generated/metafile/wasmppt_metafile_wasm_bg.wasm',
  'packages/wasmppt-worker/src/generated/shaper/wasmppt_shaper_wasm_bg.wasm',
]

function trackedBuildInputs() {
  const result = spawnSync(
    'git',
    [
      'ls-files',
      '--cached',
      '--others',
      '--exclude-standard',
      '-z',
      '--',
      'Cargo.lock',
      'Cargo.toml',
      'rust-toolchain.toml',
      'crates',
      'scripts/build-wasm-hosts.sh',
      'scripts/check-wasm-artifact-manifest.mjs',
      'tools/wasm-module.d.ts',
    ],
    { cwd: root, encoding: 'utf8' },
  )
  if (result.status !== 0) {
    throw new Error(result.stderr.trim() || 'git ls-files failed')
  }
  return result.stdout.split('\0').filter(Boolean).sort()
}

async function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex')
}

async function buildSourceHash(paths) {
  const hash = createHash('sha256')
  for (const path of paths) {
    hash.update(path)
    hash.update('\0')
    hash.update(await readFile(resolve(root, path)))
    hash.update('\0')
  }
  return hash.digest('hex')
}

async function currentManifest() {
  return {
    schema: 1,
    buildSourceSha256: await buildSourceHash(trackedBuildInputs()),
    artifacts: Object.fromEntries(
      await Promise.all(
        artifactPaths.map(async (path) => [
          relative(root, resolve(root, path)),
          await sha256(await readFile(resolve(root, path))),
        ]),
      ),
    ),
  }
}

const expected = `${JSON.stringify(await currentManifest(), null, 2)}\n`
if (process.argv.includes('--write')) {
  await writeFile(manifestPath, expected)
} else {
  const actual = await readFile(manifestPath, 'utf8').catch(() => '')
  if (actual !== expected) {
    throw new Error(
      'checked-in Wasm artifacts do not match their build inputs; run npm run build:wasm-hosts',
    )
  }
}
