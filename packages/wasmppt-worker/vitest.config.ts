import { cloudflareTest } from '@cloudflare/vitest-pool-workers'
import { readFileSync } from 'node:fs'
import { defineConfig } from 'vitest/config'

const hostFixture = [
  ...readFileSync(new URL('../../fixtures/host-adapters/minimal.potx', import.meta.url)),
]
const renderFixture = [
  ...readFileSync(new URL('../../fixtures/render/basic.pptx', import.meta.url)),
]
const dogfoodFixture = [
  ...readFileSync(new URL('../../fixtures/dogfood/report.potx', import.meta.url)),
]
const deckGateStarter = [
  ...readFileSync(new URL('../../fixtures/deck-gates/starter.potx', import.meta.url)),
]
const deckGateSpec = [
  ...readFileSync(new URL('../../fixtures/deck-gates/deck-spec.wdsf', import.meta.url)),
]
const deckGateAtomicOverflow = [
  ...readFileSync(new URL('../../fixtures/deck-gates/atomic-overflow.wdsf', import.meta.url)),
]
const performanceBudgets = JSON.parse(
  readFileSync(new URL('../../benchmarks/budgets.json', import.meta.url), 'utf8'),
)
const parityPayload = [
  ...Buffer.from(
    readFileSync(new URL('../../fixtures/host-adapters/parity.wppd.hex', import.meta.url), 'utf8')
      .trim(),
    'hex',
  ),
]

export default defineConfig({
  test: { disableConsoleIntercept: true },
  plugins: [
    cloudflareTest({
      wrangler: { configPath: './wrangler.jsonc' },
      miniflare: {
        bindings: {
          HOST_FIXTURE: hostFixture,
          RENDER_FIXTURE: renderFixture,
          DOGFOOD_FIXTURE: dogfoodFixture,
          DECK_GATE_STARTER: deckGateStarter,
          DECK_GATE_SPEC: deckGateSpec,
          DECK_GATE_ATOMIC_OVERFLOW: deckGateAtomicOverflow,
          DECK_GATE_PLAN_BUDGET_MS:
            performanceBudgets.cloudflareWorkerd.maximumDeckPlanMs,
          DECK_GATE_RESOLVE_BUDGET_MS:
            performanceBudgets.cloudflareWorkerd.maximumDeckResolveAllMs,
          DECK_GATE_EXPORT_BUDGET_MS:
            performanceBudgets.cloudflareWorkerd.maximumDeckExportMs,
          WORKER_P95_BUDGET_MS: performanceBudgets.cloudflareWorkerd.maximumWarmRequestP95Ms,
          WORKER_LIVE_P95_BUDGET_MS:
            performanceBudgets.cloudflareWorkerd.maximumLiveRequestP95Ms,
          WORKER_MEMORY_BUDGET_BYTES:
            performanceBudgets.cloudflareWorkerd.maximumAccountedMemoryBytes,
          PARITY_PAYLOAD: parityPayload,
        },
      },
    }),
  ],
})
