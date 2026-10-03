export function timingSummary(samples) {
  const sorted = samples.toSorted((left, right) => left - right)
  return {
    p50Ms: sorted[Math.ceil(sorted.length * 0.5) - 1],
    p95Ms: sorted[Math.ceil(sorted.length * 0.95) - 1],
  }
}

/** Derive eligibility from every timed output and the required sample matrix. */
export function comparisonEvaluation(processes, configuration) {
  const participants = ['cold', 'warm', 'author', 'raw-author'].map((phase) => {
    const records = processes.flatMap((process) => process.samples.filter((sample) => sample.phase === phase))
    const failures = []
    if (processes.length !== configuration.processRuns) failures.push('missing process runs')
    for (const [index, process] of processes.entries()) {
      const samples = process.samples.filter((sample) => sample.phase === phase)
      if (samples.length !== configuration.iterations || new Set(samples.map((sample) => sample.iteration)).size !== configuration.iterations ||
          samples.some((sample) => !Number.isSafeInteger(sample.iteration) || sample.iteration < 0 || sample.iteration >= configuration.iterations)) failures.push(`process ${index} has missing or duplicate iterations`)
    }
    for (const record of records) {
      if (!Number.isFinite(record.durationMs) || record.durationMs <= 0) failures.push(`invalid timing for ${record.output}`)
      if (record.validation?.openXml?.exitCode !== 0 || record.validation?.semantic?.passed !== true || !/^[0-9a-f]{64}$/.test(record.outputSha256 ?? '')) failures.push(`invalid or unverified output ${record.output}`)
    }
    const eligible = failures.length === 0
    const samplesMs = records.map((record) => record.durationMs)
    return { phase, eligible, failures, samplesMs, summary: samplesMs.every((value) => Number.isFinite(value) && value > 0) && samplesMs.length > 0 ? timingSummary(samplesMs) : null }
  })
  const cold = participants.find((participant) => participant.phase === 'cold')
  const author = participants.find((participant) => participant.phase === 'author')
  const eligible = cold.eligible && author.eligible
  return {
    participants,
    comparison: {
      eligible,
      numerator: 'PptxGenJS new-deck authoring plus measured notes-master order correction',
      denominator: 'wasmppt unprepared-template generation including preparation',
      scope: 'Equivalent requested text, page geometry, text-box geometry, typography, and background; different generation operations.',
      ratios: eligible ? {
        p50: author.summary.p50Ms / cold.summary.p50Ms,
        p95: author.summary.p95Ms / cold.summary.p95Ms,
      } : null,
      warmComparison: { eligible: false, ratios: null, reason: 'Prepared template reuse has no equivalent PptxGenJS operation. Warm timings are context only.' },
      claim: null,
    },
    passed: participants.filter((participant) => participant.phase !== 'raw-author').every((participant) => participant.eligible),
  }
}
