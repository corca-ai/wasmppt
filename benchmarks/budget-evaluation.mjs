export function evaluateNativeBudget(benchmarkReport, allBudgets) {
  const budget = allBudgets.native
  const checks = []
  const maximum = (name, actual, limit) => checks.push({
    name,
    kind: 'maximum',
    actual,
    limit,
    margin: limit - actual,
    marginPercent: limit === 0 ? null : Number((((limit - actual) / limit) * 100).toFixed(2)),
    passed: actual <= limit,
  })
  const minimum = (name, actual, limit) => checks.push({
    name,
    kind: 'minimum',
    actual,
    limit,
    margin: actual - limit,
    marginPercent: limit === 0 ? null : Number((((actual - limit) / limit) * 100).toFixed(2)),
    passed: actual >= limit,
  })
  const results = new Map(benchmarkReport.results.map((entry) => [entry.slides, entry]))
  minimum(
    'calibration.processRuns',
    benchmarkReport.configuration.processRuns,
    budget.calibration.minimumProcessRuns,
  )
  minimum(
    'calibration.timingSamplesPerProcess',
    benchmarkReport.configuration.iterationsPerProcess,
    budget.calibration.minimumTimingSamplesPerProcess,
  )
  for (const [slidesText, limits] of Object.entries(budget.matrix)) {
    const slides = Number(slidesText)
    const result = results.get(slides)
    if (result === undefined) {
      checks.push({ name: `matrix.${slides}`, passed: false, error: 'missing fixture result' })
      continue
    }
    for (const [name, limit] of Object.entries(limits.maximumP95Ns)) {
      maximum(`matrix.${slides}.p95Ns.${name}`, result.summary[name].p95Ns, limit)
    }
    maximum(`matrix.${slides}.peakResidentBytes`, result.peakResidentBytes, limits.maximumPeakResidentBytes)
    maximum(
      `matrix.${slides}.estimatedResidentBytes`,
      result.estimatedResidentBytes,
      limits.maximumEstimatedResidentBytes,
    )
    maximum(
      `matrix.${slides}.dirtyUncompressedBytes`,
      result.generation.dirtyUncompressedBytes,
      limits.maximumDirtyUncompressedBytes,
    )
    maximum(
      `matrix.${slides}.peakDirtyEntryBytes`,
      result.generation.peakDirtyEntryBytes,
      budget.invariants.maximumPeakDirtyEntryBytes,
    )
    maximum(
      `matrix.${slides}.maximumOutputChunkBytes`,
      result.generation.maximumOutputChunkBytes,
      budget.invariants.maximumOutputChunkBytes,
    )
    minimum(
      `matrix.${slides}.rawCopiedEntries`,
      result.zip.rawCopiedEntries,
      budget.invariants.minimumRawCopiedEntries,
    )
    maximum(
      `matrix.${slides}.inflatedEntries`,
      result.zip.inflatedEntries,
      budget.invariants.maximumInflatedEntries,
    )
  }

  const configuredSlides = Object.keys(budget.matrix).map(Number).toSorted((left, right) => left - right)
  if (configuredSlides.every((slides) => results.has(slides))) {
    const timingNames = Object.keys(budget.matrix[String(configuredSlides[0])].maximumP95Ns)
    const growthMetrics = {
      peakResidentBytes: (result) => result.peakResidentBytes,
      estimatedResidentBytes: (result) => result.estimatedResidentBytes,
      dirtyUncompressedBytes: (result) => result.generation.dirtyUncompressedBytes,
      outputBytes: (result) => result.outputBytes,
      livePeakResidentBytes: (result) => result.live.cache.peakResidentBytes,
      ...Object.fromEntries(timingNames.map((name) => [
        `${name}P95Ns`,
        (result) => result.summary[name].p95Ns,
      ])),
    }
    const adjacentPairs = configuredSlides.slice(1).map(
      (slides, index) => [configuredSlides[index], slides],
    )
    const growthPairs = [
      ...adjacentPairs,
      [configuredSlides[0], configuredSlides.at(-1)],
    ]
    for (const [smallSlides, largeSlides] of growthPairs) {
      const small = results.get(smallSlides)
      const large = results.get(largeSlides)
      for (const [name, read] of Object.entries(growthMetrics)) {
        const normalized = (read(large) / read(small)) / (largeSlides / smallSlides)
        maximum(
          `growth.${smallSlides}-${largeSlides}.${name}.normalized`,
          Number(normalized.toFixed(4)),
          budget.growth.maximumNormalized[name],
        )
      }
    }
  }

  maximum(
    'artifacts.scalarWasmBytes',
    benchmarkReport.artifacts.scalarWasmBytes,
    allBudgets.browserScalarWasm.maximumBinaryBytes,
  )
  maximum(
    'artifacts.metafileWasmBytes',
    benchmarkReport.artifacts.metafileWasmBytes,
    allBudgets.browserScalarWasm.maximumMetafileBinaryBytes,
  )
  maximum(
    'artifacts.shaperWasmBytes',
    benchmarkReport.artifacts.shaperWasmBytes,
    allBudgets.browserScalarWasm.maximumShaperBinaryBytes,
  )
  for (const [slides, limits] of Object.entries(allBudgets.nativeLive)) {
    const live = benchmarkReport.results.find(
      (entry) => entry.scenario === 'mixed' && entry.slides === Number(slides),
    )
    if (live === undefined) {
      checks.push({ name: `live.${slides}`, passed: false, error: 'missing fixture result' })
      continue
    }
    maximum(
      `live.${slides}.applyDeltaP95Ns`,
      live.live.summary.applyDelta.p95Ns,
      limits.maximumApplyDeltaP95Ns,
    )
    maximum(
      `live.${slides}.inputToRenderReadyP95Ns`,
      live.live.summary.inputToRenderReady.p95Ns,
      limits.maximumInputToRenderReadyP95Ns,
    )
    maximum(
      `live.${slides}.backgroundExportP95Ns`,
      live.live.summary.backgroundExport.p95Ns,
      limits.maximumBackgroundExportP95Ns,
    )
    maximum(
      `live.${slides}.invalidatedSlides`,
      live.live.maximumInvalidatedSlides,
      limits.maximumInvalidatedSlides,
    )
    maximum(
      `live.${slides}.peakResidentBytes`,
      live.live.cache.peakResidentBytes,
      limits.maximumPeakResidentBytes,
    )
    minimum(
      `live.${slides}.reusedMaterializedParts`,
      live.live.cache.minimumReusedMaterializedParts,
      limits.minimumReusedMaterializedParts,
    )
  }
  for (const [name, limits] of Object.entries(allBudgets.nativeLiveOperations)) {
    const operation = benchmarkReport.liveOperations.operations[name]
    if (operation === undefined) {
      checks.push({ name: `liveOperations.${name}`, passed: false, error: 'missing operation result' })
      continue
    }
    maximum(
      `liveOperations.${name}.applyDeltaP95Ns`,
      operation.summary.applyDelta.p95Ns,
      limits.maximumApplyDeltaP95Ns,
    )
    maximum(
      `liveOperations.${name}.inputToRenderReadyP95Ns`,
      operation.summary.inputToRenderReady.p95Ns,
      limits.maximumInputToRenderReadyP95Ns,
    )
    maximum(
      `liveOperations.${name}.invalidatedSlides`,
      operation.maximumInvalidatedSlides,
      limits.maximumInvalidatedSlides,
    )
    maximum(
      `liveOperations.${name}.residentBytes`,
      operation.maximumResidentBytes,
      limits.maximumResidentBytes,
    )
  }
  const failures = checks.filter((check) => !check.passed).map((check) =>
    `${check.name}: ${check.error ?? `${check.actual} violates ${check.kind} ${check.limit}`}`)
  return { passed: failures.length === 0, calibration: budget.calibration, checks, failures }
}
