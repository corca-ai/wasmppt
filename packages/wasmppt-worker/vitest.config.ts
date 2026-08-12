import { cloudflareTest } from '@cloudflare/vitest-pool-workers'
import { readFileSync } from 'node:fs'
import { defineConfig } from 'vitest/config'

const hostFixture = [
  ...readFileSync(new URL('../../fixtures/host-adapters/minimal.potx', import.meta.url)),
]
const renderFixture = [
  ...readFileSync(new URL('../../fixtures/render/basic.pptx', import.meta.url)),
]
const performanceBudgets = JSON.parse(
  readFileSync(new URL('../../benchmarks/budgets.json', import.meta.url), 'utf8'),
)

export default defineConfig({
  test: { disableConsoleIntercept: true },
  plugins: [
    cloudflareTest({
      wrangler: { configPath: './wrangler.jsonc' },
      miniflare: {
        bindings: {
          HOST_FIXTURE: hostFixture,
          RENDER_FIXTURE: renderFixture,
          WORKER_P95_BUDGET_MS: performanceBudgets.cloudflareWorkerd.maximumWarmRequestP95Ms,
          WORKER_MEMORY_BUDGET_BYTES:
            performanceBudgets.cloudflareWorkerd.maximumAccountedMemoryBytes,
        },
      },
    }),
  ],
})
