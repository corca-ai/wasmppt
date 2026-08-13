import {
  CanvasDisplayListRenderer,
  WasmpptError,
  WasmpptWorkerClient,
  decodeDisplayList,
  encodeInjectionData,
} from '../src/index.js'

export async function generateQuickstart(
  worker: Worker,
  template: ArrayBuffer,
  signal: AbortSignal,
): Promise<ArrayBuffer> {
  const client = new WasmpptWorkerClient(worker)
  const prepared = await client.prepare(template, { macroPolicy: 'strip' })
  try {
    return await client.generate(
      prepared.handle,
      { text: { title: 'Quarterly report' } },
      { signal },
    )
  } finally {
    await client.release(prepared.handle)
    client.terminate()
  }
}

export async function renderQuickstart(
  worker: Worker,
  bytes: ArrayBuffer,
  canvas: HTMLCanvasElement,
): Promise<void> {
  const client = new WasmpptWorkerClient(worker)
  const presentation = await client.openPresentation(bytes)
  const renderer = new CanvasDisplayListRenderer()
  try {
    const scene = decodeDisplayList(await client.resolveSlide(presentation.handle, 0))
    const context = canvas.getContext('2d')
    if (context === null) throw new Error('Canvas 2D is unavailable')
    await renderer.render(scene, context)
  } finally {
    renderer.clear()
    await client.releasePresentation(presentation.handle)
    client.terminate()
  }
}

export async function r2Quickstart(endpoint: string): Promise<ArrayBuffer> {
  const response = await fetch(`${endpoint}?r2=templates%2Freport.potx`, {
    method: 'POST',
    headers: { 'content-type': 'application/vnd.corca.wasmppt.injection-v2' },
    body: encodeInjectionData({ text: { title: 'Quarterly report' } }),
  })
  if (!response.ok) {
    const body = await response.json() as { readonly error: ConstructorParameters<typeof WasmpptError>[0] }
    throw new WasmpptError(body.error)
  }
  return response.arrayBuffer()
}
