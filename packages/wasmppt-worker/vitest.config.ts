import { cloudflareTest } from '@cloudflare/vitest-pool-workers'
import { readFileSync } from 'node:fs'
import { defineConfig } from 'vitest/config'

const hostFixture = [
  ...readFileSync(new URL('../../fixtures/host-adapters/minimal.potx', import.meta.url)),
]

export default defineConfig({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: './wrangler.jsonc' },
      miniflare: {
        bindings: { HOST_FIXTURE: hostFixture },
      },
    }),
  ],
})
