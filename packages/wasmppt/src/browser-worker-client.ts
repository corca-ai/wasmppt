import { WasmpptWorkerClient } from './worker-client.js'

export const WASMPPT_BROWSER_WORKER_READY = 'wasmppt:browser-worker-ready' as const
export const WASMPPT_BROWSER_WORKER_ERROR = 'wasmppt:browser-worker-error' as const

export interface BrowserWorkerStartupError {
  readonly message: string
  readonly type: typeof WASMPPT_BROWSER_WORKER_ERROR
}

export interface BrowserWorkerStartupReady {
  readonly type: typeof WASMPPT_BROWSER_WORKER_READY
}

export type BrowserWorkerStartupMessage = BrowserWorkerStartupError | BrowserWorkerStartupReady

/**
 * Wait for the self-initializing module Worker before exposing a request client.
 * A failed or timed-out Worker is terminated so no caller can enqueue work that
 * will remain pending forever.
 */
export function connectWasmpptBrowserWorker(
  worker: Worker,
  timeoutMs = 15_000,
): Promise<WasmpptWorkerClient> {
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs <= 0) {
    worker.terminate()
    return Promise.reject(new RangeError('timeoutMs must be a positive safe integer'))
  }

  return new Promise((resolve, reject) => {
    const timeout = setTimeout(
      () => fail(new Error('wasmppt browser Worker initialization timed out')),
      timeoutMs,
    )

    const onError = (event: ErrorEvent): void => {
      fail(new Error(event.message || 'wasmppt browser Worker failed to initialize'))
    }
    const onMessageError = (): void => {
      fail(new Error('wasmppt browser Worker startup message could not be decoded'))
    }
    const onMessage = (event: MessageEvent<unknown>): void => {
      const message = browserWorkerStartupMessage(event.data)
      if (message === null) return
      if (message.type === WASMPPT_BROWSER_WORKER_ERROR) {
        fail(new Error(message.message))
        return
      }
      cleanup()
      resolve(new WasmpptWorkerClient(worker))
    }

    function cleanup(): void {
      clearTimeout(timeout)
      worker.removeEventListener('error', onError)
      worker.removeEventListener('messageerror', onMessageError)
      worker.removeEventListener('message', onMessage)
    }

    function fail(error: Error): void {
      cleanup()
      worker.terminate()
      reject(error)
    }

    worker.addEventListener('error', onError)
    worker.addEventListener('messageerror', onMessageError)
    worker.addEventListener('message', onMessage)
  })
}

function browserWorkerStartupMessage(value: unknown): BrowserWorkerStartupMessage | null {
  if (typeof value !== 'object' || value === null || !('type' in value)) return null
  if (value.type === WASMPPT_BROWSER_WORKER_READY) {
    return { type: WASMPPT_BROWSER_WORKER_READY }
  }
  if (
    value.type === WASMPPT_BROWSER_WORKER_ERROR &&
    'message' in value &&
    typeof value.message === 'string'
  ) {
    return { type: WASMPPT_BROWSER_WORKER_ERROR, message: value.message }
  }
  return null
}
