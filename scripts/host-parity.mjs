import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export async function main(arguments_ = process.argv.slice(2)) {
  const options = Object.fromEntries(arguments_.map((argument) => {
    const match = /^--([a-z-]+)=(.+)$/.exec(argument)
    if (match === null) throw new Error(`invalid host parity argument: ${argument}`)
    return [match[1], resolve(match[2])]
  }))
  for (const required of ['native', 'browser', 'workerd-log', 'output']) {
    if (options[required] === undefined) throw new Error(`missing --${required}=PATH`)
  }
  const marker = 'PPTX_PARITY_WORKERD:'
  const log = await readFile(options['workerd-log'], 'utf8')
  const encoded = log.split('\n').find((line) => line.startsWith(marker))?.slice(marker.length)
  if (encoded === undefined) throw new Error('workerd parity bytes are absent from the test log')
  const workerd = Buffer.from(encoded, 'base64')
  const inputs = {
    native: await readFile(options.native),
    browser: await readFile(options.browser),
    workerd,
  }
  const comparisons = [
    comparePptx('native', inputs.native, 'browser', inputs.browser),
    comparePptx('native', inputs.native, 'workerd', inputs.workerd),
    comparePptx('browser', inputs.browser, 'workerd', inputs.workerd),
  ]
  const root = resolve(new URL('../', import.meta.url).pathname)
  const template = await readFile(resolve(root, 'fixtures/host-adapters/minimal.potx'))
  const payloadHex = (await readFile(
    resolve(root, 'fixtures/host-adapters/parity.wppd.hex'),
    'utf8',
  )).trim()
  const payload = Buffer.from(payloadHex, 'hex')
  const report = {
    schema: 1,
    revision: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
    fixture: {
      id: 'host-minimal-potx',
      sha256: sha256(template),
      bytes: template.byteLength,
    },
    payload: {
      path: 'fixtures/host-adapters/parity.wppd.hex',
      schema: payload.readUInt32LE(4),
      sha256: sha256(payload),
      bytes: payload.byteLength,
    },
    hosts: Object.fromEntries(Object.entries(inputs).map(([host, bytes]) => [
      host,
      { sha256: sha256(bytes), bytes: bytes.byteLength },
    ])),
    identical: comparisons.every((comparison) => comparison.identical),
    comparisons,
  }
  await mkdir(dirname(options.output), { recursive: true })
  await writeFile(options.output, `${JSON.stringify(report, null, 2)}\n`)
  await writeFile(resolve(dirname(options.output), 'workerd.pptx'), workerd)
  if (!report.identical) process.exitCode = 1
  return report
}

export function comparePptx(leftHost, leftBytes, rightHost, rightBytes) {
  const left = Buffer.from(leftBytes)
  const right = Buffer.from(rightBytes)
  if (left.equals(right)) return { left: leftHost, right: rightHost, identical: true, firstDifference: null }

  let firstDifference
  try {
    const leftZip = parseZip(left)
    const rightZip = parseZip(right)
    const count = Math.max(leftZip.entries.length, rightZip.entries.length)
    for (let index = 0; index < count && firstDifference === undefined; index += 1) {
      const before = leftZip.entries[index]
      const after = rightZip.entries[index]
      if (before?.name !== after?.name) {
        firstDifference = {
          category: 'entry-order',
          entry: before?.name ?? after?.name ?? null,
          leftEntry: before?.name ?? null,
          rightEntry: after?.name ?? null,
        }
      } else if (
        before.method !== after.method ||
        before.crc32 !== after.crc32 ||
        before.uncompressedSize !== after.uncompressedSize ||
        before.compressedSize !== after.compressedSize
      ) {
        firstDifference = {
          category: 'metadata',
          entry: before.name,
          left: metadata(before),
          right: metadata(after),
        }
      } else if (!before.localHeader.equals(after.localHeader)) {
        firstDifference = difference('headers', before.name, before.localHeader, after.localHeader)
      } else if (!before.compressed.equals(after.compressed)) {
        firstDifference = difference(
          'compressed-payload',
          before.name,
          before.compressed,
          after.compressed,
        )
      } else if (!before.centralRecord.equals(after.centralRecord)) {
        firstDifference = difference(
          'central-directory',
          before.name,
          before.centralRecord,
          after.centralRecord,
        )
      }
    }
    if (firstDifference === undefined && !leftZip.centralDirectory.equals(rightZip.centralDirectory)) {
      firstDifference = difference(
        'central-directory',
        null,
        leftZip.centralDirectory,
        rightZip.centralDirectory,
      )
    }
  } catch (error) {
    firstDifference = {
      category: 'invalid-zip',
      entry: null,
      message: error instanceof Error ? error.message : String(error),
    }
  }
  firstDifference ??= difference('archive-bytes', null, left, right)
  return { left: leftHost, right: rightHost, identical: false, firstDifference }
}

function parseZip(bytes) {
  const eocd = findEndOfCentralDirectory(bytes)
  const count = bytes.readUInt16LE(eocd + 10)
  const centralSize = bytes.readUInt32LE(eocd + 12)
  const centralOffset = bytes.readUInt32LE(eocd + 16)
  let offset = centralOffset
  const entries = []
  for (let index = 0; index < count; index += 1) {
    if (bytes.readUInt32LE(offset) !== 0x02014b50) throw new Error('invalid ZIP central directory')
    const method = bytes.readUInt16LE(offset + 10)
    const crc32 = bytes.readUInt32LE(offset + 16)
    const compressedSize = bytes.readUInt32LE(offset + 20)
    const uncompressedSize = bytes.readUInt32LE(offset + 24)
    const nameLength = bytes.readUInt16LE(offset + 28)
    const extraLength = bytes.readUInt16LE(offset + 30)
    const commentLength = bytes.readUInt16LE(offset + 32)
    const localOffset = bytes.readUInt32LE(offset + 42)
    const centralEnd = offset + 46 + nameLength + extraLength + commentLength
    const name = bytes.subarray(offset + 46, offset + 46 + nameLength).toString('utf8')
    if (bytes.readUInt32LE(localOffset) !== 0x04034b50) throw new Error(`invalid local header: ${name}`)
    const localNameLength = bytes.readUInt16LE(localOffset + 26)
    const localExtraLength = bytes.readUInt16LE(localOffset + 28)
    const dataOffset = localOffset + 30 + localNameLength + localExtraLength
    entries.push({
      name,
      method,
      crc32,
      compressedSize,
      uncompressedSize,
      localHeader: bytes.subarray(localOffset, dataOffset),
      compressed: bytes.subarray(dataOffset, dataOffset + compressedSize),
      centralRecord: bytes.subarray(offset, centralEnd),
    })
    offset = centralEnd
  }
  return {
    entries,
    centralDirectory: bytes.subarray(centralOffset, centralOffset + centralSize),
  }
}

function findEndOfCentralDirectory(bytes) {
  const minimum = Math.max(0, bytes.length - 65_557)
  for (let offset = bytes.length - 22; offset >= minimum; offset -= 1) {
    if (bytes.readUInt32LE(offset) === 0x06054b50) return offset
  }
  throw new Error('ZIP end-of-central-directory record is missing')
}

function metadata(entry) {
  return {
    method: entry.method,
    crc32: entry.crc32,
    compressedSize: entry.compressedSize,
    uncompressedSize: entry.uncompressedSize,
  }
}

function difference(category, entry, left, right) {
  const maximum = Math.min(left.length, right.length)
  let offset = 0
  while (offset < maximum && left[offset] === right[offset]) offset += 1
  return {
    category,
    entry,
    byteOffset: offset,
    leftByte: offset < left.length ? left[offset] : null,
    rightByte: offset < right.length ? right[offset] : null,
    leftLength: left.length,
    rightLength: right.length,
  }
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex')
}

const isMain = process.argv[1] !== undefined &&
  resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))
if (isMain) await main()
