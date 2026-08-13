export const CORE_PACKAGES = new Set([
  'wasmppt-deck',
  'wasmppt-deck-layout',
  'wasmppt-deck-template',
  'wasmppt-display',
  'wasmppt-layout',
  'wasmppt-metafile',
  'wasmppt-opc',
  'wasmppt-pml',
  'wasmppt-shaper',
  'wasmppt-template',
  'wasmppt-xml',
])

export const FORBIDDEN_PACKAGES = new Set([
  'js-sys',
  'wasm-bindgen',
  'wasm-bindgen-futures',
  'web-sys',
  'worker',
  'worker-kv',
  'worker-macros',
  'worker-sys',
])

/** Return readable dependency paths that connect a core crate to a host-only crate. */
export function coreBoundaryViolations(
  metadata,
  { corePackages = CORE_PACKAGES, forbiddenPackages = FORBIDDEN_PACKAGES } = {},
) {
  const packagesById = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]))
  const nodesById = new Map(metadata.resolve.nodes.map((node) => [node.id, node]))
  const roots = metadata.packages.filter((pkg) => corePackages.has(pkg.name))

  const missing = [...corePackages].filter((name) => !roots.some((pkg) => pkg.name === name))
  if (missing.length > 0) {
    throw new Error(`core boundary check is missing workspace packages: ${missing.join(', ')}`)
  }

  const violations = []
  for (const root of roots) {
    const queue = [root.id]
    const parent = new Map([[root.id, null]])

    while (queue.length > 0) {
      const id = queue.shift()
      const pkg = packagesById.get(id)
      if (!pkg) continue

      if (id !== root.id && forbiddenPackages.has(pkg.name)) {
        const path = []
        let current = id
        while (current) {
          path.push(packagesById.get(current)?.name ?? current)
          current = parent.get(current)
        }
        violations.push(path.toReversed().join(' -> '))
        continue
      }

      const node = nodesById.get(id)
      for (const dependency of node?.dependencies ?? []) {
        if (parent.has(dependency)) continue
        parent.set(dependency, id)
        queue.push(dependency)
      }
    }
  }

  return { roots: roots.map((pkg) => pkg.name), violations }
}
