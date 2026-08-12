import assert from 'node:assert/strict'
import test from 'node:test'

import { coreBoundaryViolations } from './core-boundary.mjs'

function metadata(packages, edges) {
  return {
    packages: packages.map((name) => ({ id: name, name })),
    resolve: {
      nodes: packages.map((name) => ({ id: name, dependencies: edges[name] ?? [] })),
    },
  }
}

test('accepts a host-agnostic core graph', () => {
  const result = coreBoundaryViolations(
    metadata(['core', 'xml'], { core: ['xml'] }),
    { corePackages: new Set(['core']), forbiddenPackages: new Set(['web-sys']) },
  )

  assert.deepEqual(result.violations, [])
})

test('reports the complete path to a transitive host dependency', () => {
  const result = coreBoundaryViolations(
    metadata(['core', 'middle', 'web-sys'], { core: ['middle'], middle: ['web-sys'] }),
    { corePackages: new Set(['core']), forbiddenPackages: new Set(['web-sys']) },
  )

  assert.deepEqual(result.violations, ['core -> middle -> web-sys'])
})

test('fails closed when an expected core package disappears', () => {
  assert.throws(
    () =>
      coreBoundaryViolations(metadata([], {}), {
        corePackages: new Set(['core']),
        forbiddenPackages: new Set(),
      }),
    /missing workspace packages: core/,
  )
})
