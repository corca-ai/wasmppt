export function decodeDeckPageMetadata(rows: unknown[]): import('../protocol.js').DeckPageMetadata {
  if (rows.length !== 6) throw new TypeError('invalid deck page metadata')
  const [pageId, logicalSlideId, hidden, continuationOrdinal, continuationTotal, continuationLabel] = rows
  if (!isStableId(pageId) || !isStableId(logicalSlideId) || typeof hidden !== 'boolean' ||
    !isPositiveInteger(continuationOrdinal) || !isPositiveInteger(continuationTotal) ||
    continuationOrdinal > continuationTotal ||
    !(continuationLabel === null || typeof continuationLabel === 'string')) {
    throw new TypeError('invalid deck page metadata')
  }
  return {
    pageId,
    logicalSlideId,
    hidden,
    continuationOrdinal,
    continuationTotal,
    ...(continuationLabel === null ? {} : { continuationLabel }),
  }
}

export function decodeDeckDiagnostics(
  rows: unknown[],
): import('../protocol.js').DeckDiagnostic[] {
  return rows.map((value) => {
    if (!Array.isArray(value) || value.length !== 7) {
      throw new TypeError('invalid deck diagnostic metadata')
    }
    const [code, name, severity, message, rawSource, nodeId, pageId] = value
    const source = rawSource === null ? undefined : decodeDeckDiagnosticSource(rawSource)
    if (!isNonNegativeInteger(code) || code > 0xffff ||
      !(name === null || typeof name === 'string') ||
      !(severity === 'info' || severity === 'warning' || severity === 'error') ||
      typeof message !== 'string' ||
      !(nodeId === null || typeof nodeId === 'string') ||
      !(pageId === null || typeof pageId === 'string')) {
      throw new TypeError('invalid deck diagnostic metadata')
    }
    return {
      code,
      ...(name === null ? {} : { name }),
      severity,
      message,
      ...(source === undefined ? {} : { source }),
      ...(nodeId === null ? {} : { nodeId }),
      ...(pageId === null ? {} : { pageId }),
    }
  })
}

function decodeDeckDiagnosticSource(value: unknown) {
  if (!Array.isArray(value) || value.length !== 3) {
    throw new TypeError('invalid deck diagnostic source metadata')
  }
  const [source, start, end] = value
  if (typeof source !== 'string' || source.length === 0 ||
    !isNonNegativeInteger(start) || !isNonNegativeInteger(end) || start > end) {
    throw new TypeError('invalid deck diagnostic source metadata')
  }
  return { source, start, end }
}

function isNonNegativeInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) >= 0
}

function isPositiveInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) > 0
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && /^[0-9a-f]{32}$/u.test(value)
}
