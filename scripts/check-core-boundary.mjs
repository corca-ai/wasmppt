import { execFileSync } from 'node:child_process'

import { coreBoundaryViolations } from './core-boundary.mjs'

const metadata = JSON.parse(
  execFileSync('cargo', ['metadata', '--format-version', '1', '--locked', '--all-features'], {
    encoding: 'utf8',
  }),
)

const { roots, violations } = coreBoundaryViolations(metadata)

if (violations.length > 0) {
  throw new Error(`host dependency reached from a core crate:\n${violations.join('\n')}`)
}

console.log(`core boundary ok: ${roots.length} host-agnostic crates`)
