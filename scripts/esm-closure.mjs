import { cp, mkdir, readFile } from 'node:fs/promises'
import { dirname, extname, isAbsolute, relative, resolve, sep } from 'node:path'

import { init, parse } from 'es-module-lexer'

const JAVASCRIPT_EXTENSIONS = new Set(['.js', '.mjs'])

/**
 * Copy every local runtime dependency reachable from the supplied browser module entry points.
 * Mounts map locations in the static artifact back to their build-time source directories.
 */
export async function copyEsmClosure({ entries, mounts, outputRoot }) {
  assertAbsolutePath(outputRoot, 'outputRoot')
  if (entries.length === 0) throw new Error('at least one ESM entry point is required')
  if (mounts.length === 0) throw new Error('at least one ESM source mount is required')

  const normalizedMounts = mounts
    .map(({ sourceRoot, outputRoot: mountOutputRoot }) => {
      assertAbsolutePath(sourceRoot, 'mount sourceRoot')
      assertAbsolutePath(mountOutputRoot, 'mount outputRoot')
      assertWithin(outputRoot, mountOutputRoot, 'mount outputRoot')
      return { sourceRoot, outputRoot: mountOutputRoot }
    })
    .sort((left, right) => right.outputRoot.length - left.outputRoot.length)

  const pending = entries.map((outputPath) => ({ outputPath }))
  const copied = new Set()
  while (pending.length > 0) {
    const { outputPath, importer, specifier: importedSpecifier } = pending.pop()
    assertAbsolutePath(outputPath, 'ESM entry or dependency')
    assertWithin(outputRoot, outputPath, 'ESM entry or dependency')
    if (copied.has(outputPath)) continue

    const sourcePath = sourceForOutput(outputPath, normalizedMounts)
    let source
    try {
      source = await readFile(sourcePath)
    } catch (error) {
      if (error?.code !== 'ENOENT') throw error
      const importContext = importer === undefined
        ? 'declared as an entry point'
        : `imported as ${JSON.stringify(importedSpecifier)} by ${relative(outputRoot, importer)}`
      throw new Error(
        `Local ESM dependency is missing: ${relative(outputRoot, outputPath)} ` +
        `(${importContext}; expected source ${sourcePath})`,
        { cause: error },
      )
    }
    await mkdir(dirname(outputPath), { recursive: true })
    await cp(sourcePath, outputPath)
    copied.add(outputPath)

    if (!JAVASCRIPT_EXTENSIONS.has(extname(outputPath))) continue
    for (const specifier of await runtimeDependencies(source.toString('utf8'), sourcePath)) {
      const dependency = resolveDependency(outputRoot, outputPath, specifier)
      if (dependency !== undefined && !copied.has(dependency)) {
        pending.push({ outputPath: dependency, importer: outputPath, specifier })
      }
    }
  }

  return [...copied].map((path) => relative(outputRoot, path)).sort()
}

async function runtimeDependencies(source, filename) {
  await init
  const [imports] = parse(source, filename)
  return imports.flatMap(({ d: dynamicOffset, n: specifier }) => {
    if (typeof specifier === 'string') return [specifier]
    if (dynamicOffset >= 0) {
      throw new Error(`Cannot derive computed dynamic ESM dependency in ${filename}`)
    }
    return []
  })
}

function resolveDependency(outputRoot, importer, specifier) {
  const path = specifier.split(/[?#]/u, 1)[0]
  if (path.startsWith('./') || path.startsWith('../')) {
    const dependency = resolve(dirname(importer), path)
    assertWithin(outputRoot, dependency, `dependency ${specifier} imported by ${importer}`)
    return dependency
  }
  if (path.startsWith('/')) {
    const dependency = resolve(outputRoot, `.${path}`)
    assertWithin(outputRoot, dependency, `dependency ${specifier} imported by ${importer}`)
    return dependency
  }
  if (/^(?:https?:|data:|blob:)/u.test(path)) return undefined
  throw new Error(
    `Bare ESM dependency ${JSON.stringify(specifier)} imported by ${relative(outputRoot, importer)} ` +
    'cannot be resolved in the static artifact',
  )
}

function sourceForOutput(outputPath, mounts) {
  for (const mount of mounts) {
    if (!isWithin(mount.outputRoot, outputPath)) continue
    return resolve(mount.sourceRoot, relative(mount.outputRoot, outputPath))
  }
  throw new Error(`No ESM source mount owns ${outputPath}`)
}

function assertAbsolutePath(path, label) {
  if (!isAbsolute(path)) throw new Error(`${label} must be an absolute path: ${path}`)
}

function assertWithin(root, path, label) {
  if (!isWithin(root, path)) throw new Error(`${label} escapes the static artifact: ${path}`)
}

function isWithin(root, path) {
  const child = relative(root, path)
  return child === '' || (child !== '..' && !child.startsWith(`..${sep}`) && !isAbsolute(child))
}
