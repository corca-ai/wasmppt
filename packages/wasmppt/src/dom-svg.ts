import {
  ByteBudgetLru,
  FontResolver,
  buildRichTextLayout,
  decodeDisplayList,
  type DisplayScene,
  type EmuRect,
  type RichTextLayoutRun,
  type SceneCommand,
  type SceneDiagnostic,
  type SceneImage,
  type ScenePathCommand,
  type SceneSemanticElement,
  type SceneTransform,
} from './canvas.js'
import {
  EMU_PER_CSS_PIXEL,
  cssMatrix,
  groupSvgTransform,
  groupTransformMatrix,
  identityMatrix,
  multiplyMatrices,
  presetGeometryPath,
  presetGeometrySvgPath,
  shapeSvgTransform,
  toCssPixels,
  translation,
} from './scene/geometry.js'

const SVG_NAMESPACE = 'http://www.w3.org/2000/svg'

export type DomImageResolver = (image: SceneImage, signal: AbortSignal) => Promise<string>

export interface DomSvgRenderOptions {
  readonly revision: number
  readonly slideIndex?: number | undefined
  readonly signal?: AbortSignal | undefined
  readonly imageResolver?: DomImageResolver | undefined
  readonly fontResolver?: FontResolver | undefined
}

export interface DomSvgRenderResult {
  readonly root: HTMLElement
  readonly revision: number
  readonly updatedElements: number
  readonly diagnostics: readonly SceneDiagnostic[]
  readonly stale: boolean
}

interface SlideDomState {
  readonly root: HTMLDivElement
  readonly svg: SVGSVGElement
  readonly definitions: SVGDefsElement
  readonly graphics: SVGGElement
  readonly textLayer: HTMLDivElement
  readonly graphicElements: Map<string, SVGGElement>
  readonly textElements: Map<string, HTMLElement>
  revision: number
}

/** Projects the shared DisplayScene into selectable HTML text and inline SVG graphics. */
export class DomSvgRenderer {
  readonly #states = new WeakMap<HTMLElement, SlideDomState>()

  async render(
    scene: DisplayScene,
    host: HTMLElement,
    options: DomSvgRenderOptions,
  ): Promise<DomSvgRenderResult> {
    const signal = options.signal ?? new AbortController().signal
    throwIfAborted(signal)
    let state = this.#states.get(host)
    if (state !== undefined && options.revision < state.revision) {
      return Object.freeze({
        root: state.root,
        revision: state.revision,
        updatedElements: 0,
        diagnostics: scene.diagnostics,
        stale: true,
      })
    }
    if (state !== undefined && options.revision === state.revision) {
      return Object.freeze({
        root: state.root,
        revision: state.revision,
        updatedElements: 0,
        diagnostics: scene.diagnostics,
        stale: false,
      })
    }
    if (state === undefined) {
      state = createState(host)
      this.#states.set(host, state)
    }
    state.revision = options.revision
    configureRoot(state, scene, options.slideIndex)
    updateBackground(state, scene)
    state.definitions.replaceChildren()
    const retained = new Set<string>()
    let updatedElements = 0
    for (const semantic of [...scene.semantics].sort((left, right) => left.zOrder - right.zOrder)) {
      throwIfAborted(signal)
      retained.add(semanticKey(semantic))
      const group = graphicElement(state, semantic)
      group.replaceChildren()
      const commands = scene.commands.slice(
        semantic.firstCommand,
        semantic.firstCommand + semantic.commandCount,
      )
      const richTextCommand = commands.find(
        (command): command is Extract<SceneCommand, { readonly kind: 'draw-rich-text' }> =>
          command.kind === 'draw-rich-text',
      )
      let richTextLayout: Awaited<ReturnType<typeof buildRichTextLayout>> | undefined
      if (richTextCommand !== undefined) {
        const measureCanvas = document.createElement('canvas')
        const measureContext = measureCanvas.getContext('2d')
        if (measureContext === null) throw new Error('Canvas 2D is unavailable for rich text layout')
        richTextLayout = await buildRichTextLayout(measureContext, richTextCommand, options.fontResolver)
      }
      const resizedBounds = richTextCommand?.frame.autofit === 'resize-shape'
        ? richTextLayout?.effectiveBounds
        : undefined
      await renderGraphicCommands(
        state,
        group,
        commands,
        scene,
        semantic,
        options.imageResolver,
        signal,
        resizedBounds,
      )
      await updateAccessibleOverlay(
        state,
        semantic,
        commands,
        scene,
        options.fontResolver,
        richTextLayout,
        options.imageResolver,
        signal,
      )
      updatedElements += 1
    }
    removeMissing(state.graphicElements, retained)
    removeMissing(state.textElements, retained)
    for (const semantic of [...scene.semantics].sort(
      (left, right) => left.readingOrder - right.readingOrder || left.zOrder - right.zOrder,
    )) {
      const element = state.textElements.get(semanticKey(semantic))
      if (element !== undefined) state.textLayer.append(element)
    }
    return Object.freeze({
      root: state.root,
      revision: options.revision,
      updatedElements,
      diagnostics: scene.diagnostics,
      stale: false,
    })
  }

  clear(host: HTMLElement): void {
    const state = this.#states.get(host)
    if (state === undefined) return
    state.root.remove()
    this.#states.delete(host)
  }
}

function createState(host: HTMLElement): SlideDomState {
  const root = document.createElement('div')
  root.className = 'wasmppt-dom-slide'
  root.setAttribute('role', 'group')
  root.setAttribute('aria-roledescription', 'slide')
  Object.assign(root.style, { position: 'relative', overflow: 'hidden' })
  const svg = document.createElementNS(SVG_NAMESPACE, 'svg')
  svg.setAttribute('aria-hidden', 'true')
  Object.assign(svg.style, { position: 'absolute', inset: '0', width: '100%', height: '100%' })
  const definitions = document.createElementNS(SVG_NAMESPACE, 'defs')
  const graphics = document.createElementNS(SVG_NAMESPACE, 'g')
  svg.append(definitions, graphics)
  const textLayer = document.createElement('div')
  textLayer.className = 'wasmppt-dom-text-layer'
  Object.assign(textLayer.style, { position: 'absolute', inset: '0', overflow: 'hidden' })
  root.append(svg, textLayer)
  host.replaceChildren(root)
  return {
    root,
    svg,
    definitions,
    graphics,
    textLayer,
    graphicElements: new Map(),
    textElements: new Map(),
    revision: -1,
  }
}

function configureRoot(state: SlideDomState, scene: DisplayScene, slideIndex = 0): void {
  const width = scene.width / EMU_PER_CSS_PIXEL
  const height = scene.height / EMU_PER_CSS_PIXEL
  state.root.style.width = `${width}px`
  state.root.style.height = `${height}px`
  state.root.dataset['slideIndex'] = String(slideIndex)
  state.root.setAttribute('aria-label', `Slide ${slideIndex + 1}`)
  state.svg.setAttribute('viewBox', `0 0 ${scene.width} ${scene.height}`)
}

function updateBackground(state: SlideDomState, scene: DisplayScene): void {
  let background = state.svg.querySelector<SVGRectElement>(':scope > rect[data-background]')
  if (background === null) {
    background = document.createElementNS(SVG_NAMESPACE, 'rect')
    background.dataset['background'] = ''
    state.svg.insertBefore(background, state.definitions)
  }
  const clear = scene.commands.find(
    (command): command is Extract<SceneCommand, { readonly kind: 'clear' }> => command.kind === 'clear',
  )
  setAttributes(background, {
    x: 0,
    y: 0,
    width: scene.width,
    height: scene.height,
    fill: clear === undefined ? 'transparent' : cssColor(clear.color),
  })
}

function graphicElement(state: SlideDomState, semantic: SceneSemanticElement): SVGGElement {
  const key = semanticKey(semantic)
  let group = state.graphicElements.get(key)
  if (group === undefined) {
    group = document.createElementNS(SVG_NAMESPACE, 'g')
    state.graphicElements.set(key, group)
  }
  group.dataset['shapeId'] = String(semantic.shapeId)
  group.dataset['semanticKind'] = semantic.kind
  group.dataset['commandFirst'] = String(semantic.firstCommand)
  group.dataset['commandCount'] = String(semantic.commandCount)
  group.setAttribute('aria-label', semantic.alternativeText ?? semantic.name)
  group.setAttribute(
    'role',
    semantic.kind === 'image' || semantic.kind === 'table' || semantic.kind === 'chart'
      ? 'img'
      : 'graphics-symbol',
  )
  state.graphics.append(group)
  return group
}

async function renderGraphicCommands(
  state: SlideDomState,
  root: SVGGElement,
  commands: readonly SceneCommand[],
  scene: DisplayScene,
  semantic: SceneSemanticElement,
  imageResolver: DomImageResolver | undefined,
  signal: AbortSignal,
  resizedBounds?: EmuRect,
): Promise<void> {
  const stack: SVGGElement[] = [root]
  for (const command of commands) {
    throwIfAborted(signal)
    const parent = stack.at(-1)!
    if (command.kind === 'push-group') {
      const nested = document.createElementNS(SVG_NAMESPACE, 'g')
      nested.setAttribute('transform', groupSvgTransform(required(scene.groups, command.transform)))
      parent.append(nested)
      stack.push(nested)
    } else if (command.kind === 'pop-group') {
      if (stack.length > 1) stack.pop()
    } else if (command.kind === 'fill-preset' || command.kind === 'stroke-preset') {
      const transform = resizedTransform(command.transform, resizedBounds)
      const path = document.createElementNS(SVG_NAMESPACE, 'path')
      path.setAttribute('d', presetPath(command.geometry, transform.bounds.width, transform.bounds.height))
      path.setAttribute('transform', shapeSvgTransform(transform))
      if (command.kind === 'fill-preset') {
        setAttributes(path, { fill: cssColor(command.color), stroke: 'none' })
      } else {
        const headMarker = command.headEnd === undefined
          ? undefined
          : appendLineEndMarker(state.definitions, `wasmppt-head-${semantic.shapeId}-${state.definitions.childElementCount}`, command.headEnd, command.color)
        const tailMarker = command.tailEnd === undefined
          ? undefined
          : appendLineEndMarker(state.definitions, `wasmppt-tail-${semantic.shapeId}-${state.definitions.childElementCount}`, command.tailEnd, command.color)
        setAttributes(path, {
          fill: 'none',
          stroke: cssColor(command.color),
          'stroke-width': command.width,
          'stroke-dasharray': dashPattern(command.dash, command.width),
          'marker-start': headMarker === undefined ? undefined : `url(#${headMarker})`,
          'marker-end': tailMarker === undefined ? undefined : `url(#${tailMarker})`,
        })
      }
      parent.append(path)
    } else if (command.kind === 'fill-gradient-preset') {
      const transform = resizedTransform(command.transform, resizedBounds)
      const path = document.createElementNS(SVG_NAMESPACE, 'path')
      const gradientId = appendLinearGradient(
        state.definitions,
        `wasmppt-gradient-${semantic.shapeId}-${state.definitions.childElementCount}`,
        command.angle,
        command.stops,
      )
      path.setAttribute('d', presetPath(command.geometry, transform.bounds.width, transform.bounds.height))
      path.setAttribute('transform', shapeSvgTransform(transform))
      setAttributes(path, { fill: `url(#${gradientId})`, stroke: 'none' })
      parent.append(path)
    } else if (command.kind === 'fill-radial-gradient-preset') {
      const transform = resizedTransform(command.transform, resizedBounds)
      const path = document.createElementNS(SVG_NAMESPACE, 'path')
      const gradientId = appendRadialGradient(
        state.definitions,
        `wasmppt-radial-${semantic.shapeId}-${state.definitions.childElementCount}`,
        command.stops,
      )
      path.setAttribute('d', presetPath(command.geometry, transform.bounds.width, transform.bounds.height))
      path.setAttribute('transform', shapeSvgTransform(transform))
      setAttributes(path, { fill: `url(#${gradientId})`, stroke: 'none' })
      parent.append(path)
    } else if (command.kind === 'fill-pattern-preset') {
      const transform = resizedTransform(command.transform, resizedBounds)
      const path = document.createElementNS(SVG_NAMESPACE, 'path')
      const patternId = appendPattern(
        state.definitions,
        `wasmppt-pattern-${semantic.shapeId}-${state.definitions.childElementCount}`,
        command.preset,
        command.foreground,
        command.background,
      )
      path.setAttribute('d', presetPath(command.geometry, transform.bounds.width, transform.bounds.height))
      path.setAttribute('transform', shapeSvgTransform(transform))
      setAttributes(path, { fill: `url(#${patternId})`, stroke: 'none' })
      parent.append(path)
    } else if (command.kind === 'draw-custom-path') {
      const path = document.createElementNS(SVG_NAMESPACE, 'path')
      path.setAttribute('d', customPathData(command.path))
      const bounds = resizedBounds ?? command.transform.bounds
      path.setAttribute('transform', `translate(${bounds.x} ${bounds.y}) scale(${bounds.width / command.pathWidth} ${bounds.height / command.pathHeight})`)
      let customFill = 'none'
      if (command.fill.kind === 'solid') customFill = cssColor(command.fill.color)
      else if (command.fill.kind === 'linear-gradient') {
        const gradientId = appendLinearGradient(
          state.definitions,
          `wasmppt-custom-gradient-${semantic.shapeId}-${state.definitions.childElementCount}`,
          command.fill.angle,
          command.fill.stops,
        )
        customFill = `url(#${gradientId})`
      } else if (command.fill.kind === 'radial-gradient') {
        const gradientId = appendRadialGradient(
          state.definitions,
          `wasmppt-custom-radial-${semantic.shapeId}-${state.definitions.childElementCount}`,
          command.fill.stops,
        )
        customFill = `url(#${gradientId})`
      } else if (command.fill.kind === 'pattern') {
        const patternId = appendPattern(
          state.definitions,
          `wasmppt-custom-pattern-${semantic.shapeId}-${state.definitions.childElementCount}`,
          command.fill.preset,
          command.fill.foreground,
          command.fill.background,
        )
        customFill = `url(#${patternId})`
      }
      setAttributes(path, {
        fill: customFill,
        stroke: command.stroke === undefined ? 'none' : cssColor(command.stroke.color),
        'stroke-width': command.stroke?.width ?? 0,
        'vector-effect': 'non-scaling-stroke',
      })
      parent.append(path)
    } else if (command.kind === 'draw-outer-shadow') {
      const transform = resizedTransform(command.transform, resizedBounds)
      const path = document.createElementNS(SVG_NAMESPACE, 'path')
      const filter = document.createElementNS(SVG_NAMESPACE, 'filter')
      filter.id = `wasmppt-shadow-${semantic.shapeId}-${state.definitions.childElementCount}`
      setAttributes(filter, { x: '-50%', y: '-50%', width: '200%', height: '200%' })
      const blur = document.createElementNS(SVG_NAMESPACE, 'feGaussianBlur')
      blur.setAttribute('stdDeviation', String(command.blurRadius / EMU_PER_CSS_PIXEL))
      filter.append(blur)
      state.definitions.append(filter)
      const radians = command.direction / 60_000 * Math.PI / 180
      const bounds = transform.bounds
      path.setAttribute('d', presetPath(command.geometry, bounds.width, bounds.height))
      path.setAttribute('transform', `${shapeSvgTransform(transform)} translate(${Math.cos(radians) * command.distance} ${Math.sin(radians) * command.distance})`)
      setAttributes(path, { fill: cssColor(command.color), filter: `url(#${filter.id})` })
      parent.append(path)
    } else if (command.kind === 'draw-image') {
      const resource = required(scene.images, command.resource)
      const image = document.createElementNS(SVG_NAMESPACE, 'image')
      const href = imageResolver === undefined ? undefined : await imageResolver(resource, signal)
      const bounds = command.transform.bounds
      const [leftRaw, topRaw, rightRaw, bottomRaw] = command.crop
      const left = Math.max(0, leftRaw / 100_000)
      const top = Math.max(0, topRaw / 100_000)
      const right = Math.max(0, rightRaw / 100_000)
      const bottom = Math.max(0, bottomRaw / 100_000)
      const visibleWidth = Math.max(0.000_001, 1 - left - right)
      const visibleHeight = Math.max(0.000_001, 1 - top - bottom)
      const expandedWidth = bounds.width / visibleWidth
      const expandedHeight = bounds.height / visibleHeight
      const clipId = `wasmppt-clip-${semantic.shapeId}`
      const clip = document.createElementNS(SVG_NAMESPACE, 'clipPath')
      clip.id = `${clipId}-${semantic.zOrder}`
      clip.setAttribute('clipPathUnits', 'userSpaceOnUse')
      const clipRect = document.createElementNS(SVG_NAMESPACE, 'rect')
      setAttributes(clipRect, { x: 0, y: 0, width: bounds.width, height: bounds.height })
      clip.append(clipRect)
      state.definitions.append(clip)
      setAttributes(image, {
        href,
        x: -left * expandedWidth,
        y: -top * expandedHeight,
        width: expandedWidth,
        height: expandedHeight,
        preserveAspectRatio: 'none',
        transform: shapeSvgTransform(command.transform),
        'clip-path': `url(#${clip.id})`,
      })
      parent.append(image)
    } else if (command.kind === 'draw-unsupported') {
      appendUnsupportedGraphic(
        parent,
        resizedTransform(command.transform, resizedBounds),
        unsupportedGraphicLabel(command.feature),
      )
    }
  }
}

function appendLinearGradient(
  definitions: SVGDefsElement,
  id: string,
  angle: number,
  stops: readonly {
    readonly position: number
    readonly color: { readonly red: number; readonly green: number; readonly blue: number; readonly alpha: number }
  }[],
): string {
  const gradient = document.createElementNS(SVG_NAMESPACE, 'linearGradient')
  gradient.id = id
  const radians = angle / 60_000 * Math.PI / 180
  setAttributes(gradient, {
    x1: `${50 - Math.cos(radians) * 50}%`,
    y1: `${50 - Math.sin(radians) * 50}%`,
    x2: `${50 + Math.cos(radians) * 50}%`,
    y2: `${50 + Math.sin(radians) * 50}%`,
  })
  for (const value of stops) {
    const stop = document.createElementNS(SVG_NAMESPACE, 'stop')
    setAttributes(stop, {
      offset: `${value.position / 1_000}%`,
      'stop-color': cssColor(value.color),
    })
    gradient.append(stop)
  }
  definitions.append(gradient)
  return id
}

function appendRadialGradient(
  definitions: SVGDefsElement,
  id: string,
  stops: readonly { readonly position: number; readonly color: Parameters<typeof cssColor>[0] }[],
): string {
  const gradient = document.createElementNS(SVG_NAMESPACE, 'radialGradient')
  gradient.id = id
  for (const value of stops) {
    const stop = document.createElementNS(SVG_NAMESPACE, 'stop')
    setAttributes(stop, { offset: `${value.position / 1_000}%`, 'stop-color': cssColor(value.color) })
    gradient.append(stop)
  }
  definitions.append(gradient)
  return id
}

function appendPattern(
  definitions: SVGDefsElement,
  id: string,
  preset: string,
  foreground: Parameters<typeof cssColor>[0],
  background: Parameters<typeof cssColor>[0],
): string {
  const pattern = document.createElementNS(SVG_NAMESPACE, 'pattern')
  setAttributes(pattern, { id, width: 8, height: 8, patternUnits: 'userSpaceOnUse' })
  const backdrop = document.createElementNS(SVG_NAMESPACE, 'rect')
  setAttributes(backdrop, { width: 8, height: 8, fill: cssColor(background) })
  pattern.append(backdrop)
  const path = document.createElementNS(SVG_NAMESPACE, 'path')
  const vertical = /Vert|vert/u.test(preset)
  const horizontal = /Horz|horz/u.test(preset)
  path.setAttribute('d', vertical && horizontal
    ? 'M 4 0 V 8 M 0 4 H 8'
    : vertical
      ? 'M 4 0 V 8'
      : horizontal
        ? 'M 0 4 H 8'
        : /DnDiag|dnDiag/u.test(preset)
          ? 'M -2 0 L 8 10 M 6 -2 L 10 2'
          : 'M -2 8 L 8 -2 M 6 10 L 10 6')
  setAttributes(path, { fill: 'none', stroke: cssColor(foreground), 'stroke-width': 1 })
  pattern.append(path)
  definitions.append(pattern)
  return id
}

function appendLineEndMarker(
  definitions: SVGDefsElement,
  id: string,
  kind: 'triangle' | 'stealth' | 'diamond' | 'oval' | 'arrow',
  color: { readonly red: number; readonly green: number; readonly blue: number; readonly alpha: number },
): string {
  const marker = document.createElementNS(SVG_NAMESPACE, 'marker')
  setAttributes(marker, { id, markerWidth: 8, markerHeight: 8, refX: 7, refY: 4, orient: 'auto-start-reverse', markerUnits: 'strokeWidth' })
  const shape = kind === 'oval'
    ? document.createElementNS(SVG_NAMESPACE, 'ellipse')
    : document.createElementNS(SVG_NAMESPACE, 'path')
  if (kind === 'oval') setAttributes(shape, { cx: 4, cy: 4, rx: 3, ry: 2.5 })
  else shape.setAttribute('d', kind === 'diamond' ? 'M0 4 4 0 8 4 4 8Z' : 'M0 0 8 4 0 8 2 4Z')
  shape.setAttribute('fill', cssColor(color))
  marker.append(shape)
  definitions.append(marker)
  return id
}

function unsupportedGraphicLabel(
  feature: Extract<SceneCommand, { readonly kind: 'draw-unsupported' }>['feature'],
): string {
  if (feature === 'smartart') return 'SmartArt preview unavailable'
  if (feature === 'metafile') return 'EMF/WMF preview unavailable'
  if (feature === 'ole-object') return 'Embedded object preview unavailable'
  return 'Graphic preview unavailable'
}

function appendUnsupportedGraphic(
  parent: SVGGElement,
  transform: SceneTransform,
  label: string,
): void {
  const group = document.createElementNS(SVG_NAMESPACE, 'g')
  group.setAttribute('transform', shapeSvgTransform(transform))
  const rect = document.createElementNS(SVG_NAMESPACE, 'rect')
  setAttributes(rect, {
    x: 0,
    y: 0,
    width: transform.bounds.width,
    height: transform.bounds.height,
    fill: 'rgba(104, 116, 129, 0.08)',
    stroke: 'rgba(104, 116, 129, 0.65)',
    'stroke-width': EMU_PER_CSS_PIXEL,
    'stroke-dasharray': `${EMU_PER_CSS_PIXEL * 6} ${EMU_PER_CSS_PIXEL * 4}`,
  })
  const text = document.createElementNS(SVG_NAMESPACE, 'text')
  setAttributes(text, {
    x: transform.bounds.width / 2,
    y: transform.bounds.height / 2,
    fill: 'rgba(70, 79, 89, 0.9)',
    'font-size': EMU_PER_CSS_PIXEL * 12,
    'font-family': 'sans-serif',
    'text-anchor': 'middle',
    'dominant-baseline': 'middle',
  })
  text.textContent = label
  group.append(rect, text)
  parent.append(group)
}

async function updateAccessibleOverlay(
  state: SlideDomState,
  semantic: SceneSemanticElement,
  commands: readonly SceneCommand[],
  scene: DisplayScene,
  fontResolver: FontResolver | undefined,
  richTextLayout?: Awaited<ReturnType<typeof buildRichTextLayout>>,
  imageResolver?: DomImageResolver,
  signal: AbortSignal = new AbortController().signal,
): Promise<void> {
  const textCommand = commands.find(
    (command): command is Extract<SceneCommand, { readonly kind: 'draw-text' }> =>
      command.kind === 'draw-text',
  )
  const richTextCommand = commands.find(
    (command): command is Extract<SceneCommand, { readonly kind: 'draw-rich-text' }> =>
      command.kind === 'draw-rich-text',
  )
  if (
    textCommand === undefined && richTextCommand === undefined &&
    semantic.hyperlink === undefined &&
    semantic.alternativeText === undefined
  ) {
    const key = semanticKey(semantic)
    state.textElements.get(key)?.remove()
    state.textElements.delete(key)
    return
  }
  const safeHref = safeHyperlink(semantic.hyperlink)
  const requiredTag = safeHref === undefined ? 'div' : 'a'
  const key = semanticKey(semantic)
  let element = state.textElements.get(key)
  if (element === undefined || element.localName !== requiredTag) {
    element?.remove()
    element = document.createElement(requiredTag)
    state.textElements.set(key, element)
  }
  element.dataset['shapeId'] = String(semantic.shapeId)
  element.dataset['semanticKind'] = semantic.kind
  element.dataset['selectionId'] = semantic.source?.semanticId ?? `shape:${semantic.zOrder}:${semantic.shapeId}`
  element.dataset['readingOrder'] = String(semantic.readingOrder)
  if (semantic.source !== undefined) {
    element.dataset['source'] = semantic.source.source
    element.dataset['sourceStart'] = String(semantic.source.start)
    element.dataset['sourceEnd'] = String(semantic.source.end)
  }
  element.dataset['commandFirst'] = String(semantic.firstCommand)
  element.dataset['commandCount'] = String(semantic.commandCount)
  if (semantic.hyperlink !== undefined) element.dataset['hyperlink'] = semantic.hyperlink
  if (element instanceof HTMLAnchorElement && safeHref !== undefined) element.href = safeHref
  if (
    !(element instanceof HTMLAnchorElement) &&
    (semantic.kind === 'image' || semantic.kind === 'table' || semantic.kind === 'chart')
  ) {
    element.setAttribute('role', 'img')
  } else {
    element.removeAttribute('role')
  }
  element.setAttribute('aria-label', semantic.alternativeText ?? semantic.name)
  if (textCommand !== undefined) element.textContent = required(scene.strings, textCommand.text)
  else if (richTextCommand !== undefined) {
    let plan = richTextLayout
    if (plan === undefined) {
      const measureCanvas = document.createElement('canvas')
      const measureContext = measureCanvas.getContext('2d')
      if (measureContext === null) throw new Error('Canvas 2D is unavailable for rich text layout')
      plan = await buildRichTextLayout(measureContext, richTextCommand, fontResolver)
    }
    const originX = richTextCommand.bounds.x / EMU_PER_CSS_PIXEL
    const originY = richTextCommand.bounds.y / EMU_PER_CSS_PIXEL
    const layoutX = plan.layoutBounds.x / EMU_PER_CSS_PIXEL
    const layoutY = plan.layoutBounds.y / EMU_PER_CSS_PIXEL
    const wrapper = document.createElement('span')
    Object.assign(wrapper.style, {
      position: 'absolute',
      left: `${layoutX - originX}px`,
      top: `${layoutY - originY}px`,
      width: `${plan.layoutBounds.width / EMU_PER_CSS_PIXEL}px`,
      height: `${plan.layoutBounds.height / EMU_PER_CSS_PIXEL}px`,
      transformOrigin: '50% 50%',
      transform: plan.rotationDegrees === 0 ? 'none' : `rotate(${plan.rotationDegrees}deg)`,
    })
    const runElements = await Promise.all(plan.runs.map(async (run) => {
      const runHref = safeHref === undefined ? safeHyperlink(run.hyperlink) : undefined
      const span = document.createElement(runHref === undefined ? 'span' : 'a')
      if (run.bulletImageResource !== undefined && imageResolver !== undefined) {
        const image = document.createElement('img')
        image.src = await imageResolver(required(scene.images, run.bulletImageResource), signal)
        image.alt = ''
        Object.assign(image.style, {
          width: `${run.fontSize}px`,
          height: `${run.fontSize}px`,
          objectFit: 'contain',
          verticalAlign: 'baseline',
        })
        span.replaceChildren(image)
      } else span.textContent = run.text
      Object.assign(span.style, {
        position: 'absolute',
        left: `${run.x - layoutX}px`,
        top: `${run.baseline - layoutY}px`,
        transform: `translateY(-0.82em) rotate(${run.warpRotation}deg)`,
        font: run.font.css,
        ...domTextFill(run),
        direction: run.direction,
        letterSpacing: `${run.characterSpacing}px`,
        textDecoration: [run.underline ? 'underline' : '', run.strike ? 'line-through' : '']
          .filter(Boolean)
          .join(' ') || 'none',
        whiteSpace: 'pre',
        WebkitTextStroke: run.outline === undefined
          ? 'initial'
          : `${run.outline.width / EMU_PER_CSS_PIXEL}px ${cssColor(run.outline.color)}`,
        textShadow: [run.shadow, run.innerShadow].filter((shadow) => shadow !== undefined)
          .map((shadow) => {
            const radians = shadow.direction / 60_000 * Math.PI / 180
            return `${Math.cos(radians) * shadow.distance / EMU_PER_CSS_PIXEL}px ${Math.sin(radians) * shadow.distance / EMU_PER_CSS_PIXEL}px ${shadow.blurRadius / EMU_PER_CSS_PIXEL}px ${cssColor(shadow.color)}`
          }).join(', ') || 'none',
        filter: [
          run.glow === undefined ? '' : `drop-shadow(0 0 ${run.glow.radius / EMU_PER_CSS_PIXEL}px ${cssColor(run.glow.color)})`,
          Math.max(run.blurRadius, run.softEdgeRadius) <= 0 ? '' : `blur(${Math.max(run.blurRadius, run.softEdgeRadius) / EMU_PER_CSS_PIXEL}px)`,
        ].filter(Boolean).join(' ') || 'none',
        WebkitBoxReflect: run.reflection
          ? 'below 0 linear-gradient(transparent, rgba(0, 0, 0, 0.22))'
          : 'initial',
      })
      span.dataset.paragraphIndex = String(run.paragraphIndex)
      if (run.hyperlink !== undefined) span.dataset.hyperlink = run.hyperlink
      if (span instanceof HTMLAnchorElement && runHref !== undefined) {
        span.href = runHref
        span.style.pointerEvents = 'auto'
      }
      if (run.sourceStart !== undefined) span.dataset.sourceStart = String(run.sourceStart)
      if (run.sourceEnd !== undefined) span.dataset.sourceEnd = String(run.sourceEnd)
      return span
    }))
    wrapper.replaceChildren(...runElements)
    element.replaceChildren(wrapper)
  } else element.textContent = ''
  const groupTransforms = commands
    .filter(
      (command): command is Extract<SceneCommand, { readonly kind: 'push-group' }> =>
        command.kind === 'push-group',
    )
    .map((command) => required(scene.groups, command.transform))
  const matrix = groupTransforms.reduce(
    (current, group) => multiplyMatrices(current, groupTransformMatrix(group)),
    identityMatrix(),
  )
  const bounds = textCommand?.bounds ?? richTextLayout?.effectiveBounds ?? richTextCommand?.bounds ?? semantic.bounds
  const richFirstStyle = richTextCommand?.frame.paragraphs[0]?.runs[0]?.style
  const textStyle = textCommand?.style ?? richFirstStyle
  const positioned = multiplyMatrices(matrix, translation(bounds.x, bounds.y))
  Object.assign(element.style, {
    position: 'absolute',
    left: '0',
    top: '0',
    width: `${bounds.width / EMU_PER_CSS_PIXEL}px`,
    height: `${bounds.height / EMU_PER_CSS_PIXEL}px`,
    transformOrigin: '0 0',
    transform: cssMatrix(toCssPixels(positioned)),
    boxSizing: 'border-box',
    display: richTextCommand === undefined ? 'flex' : 'block',
    flexDirection: 'column',
    justifyContent:
      textStyle?.verticalAlignment === 'center'
        ? 'center'
        : textStyle?.verticalAlignment === 'bottom'
          ? 'flex-end'
          : 'flex-start',
    padding: richTextCommand !== undefined
      ? '0'
      : textStyle === undefined
      ? '0'
      : `${textStyle.marginTop / EMU_PER_CSS_PIXEL}px ${textStyle.marginRight / EMU_PER_CSS_PIXEL}px ${textStyle.marginBottom / EMU_PER_CSS_PIXEL}px ${textStyle.marginLeft / EMU_PER_CSS_PIXEL}px`,
    color:
      textStyle === undefined
        ? '#000'
        : `rgba(${textStyle.color.red}, ${textStyle.color.green}, ${textStyle.color.blue}, ${textStyle.color.alpha / 255})`,
    fontFamily: textStyle?.fontFamily ?? 'sans-serif',
    fontSize: `${textStyle === undefined ? 18 : (textStyle.fontSize / 100) * 96 / 72}px`,
    fontWeight: textStyle?.bold === true ? '700' : '400',
    fontStyle: textStyle?.italic === true ? 'italic' : 'normal',
    lineHeight: '1.2',
    textAlign: textStyle?.alignment === 'justify' || textStyle?.alignment === 'distributed'
      ? 'justify'
      : (textStyle?.alignment ?? 'left'),
    whiteSpace: 'pre-wrap',
    overflow: 'hidden',
    userSelect: 'text',
    pointerEvents: safeHref === undefined ? 'none' : 'auto',
  })
  state.textLayer.append(element)
}

function domTextFill(run: RichTextLayoutRun): Record<string, string> {
  const fill = run.fill
  if (fill === undefined) return { color: cssColor(run.color) }
  if (fill.kind === 'none') return { color: 'transparent' }
  if (fill.kind === 'solid') return { color: cssColor(fill.color) }
  if (fill.kind === 'pattern') return { color: cssColor(fill.foreground) }
  const stops = fill.stops
    .map((stop) => `${cssColor(stop.color)} ${stop.position / 1_000}%`)
    .join(', ')
  return {
    color: 'transparent',
    backgroundImage: fill.kind === 'linear-gradient'
      ? `linear-gradient(${fill.angle / 60_000}deg, ${stops})`
      : `radial-gradient(circle, ${stops})`,
    backgroundClip: 'text',
    WebkitBackgroundClip: 'text',
  }
}

export interface VirtualizedDomViewerOptions {
  readonly sceneCacheBytes?: number | undefined
  readonly prefetchNeighbors?: number | undefined
  readonly imageResolver?: DomImageResolver | undefined
  readonly fontResolver?: FontResolver | undefined
}

export interface DomSceneResolver {
  resolveSlide(
    presentationHandle: number,
    slideIndex: number,
    options?: { readonly signal?: AbortSignal },
  ): Promise<ArrayBuffer>
}

/** Bounded DOM/SVG slide virtualization using the same Worker scene resolver as Canvas. */
export class VirtualizedDomViewer {
  readonly #resolver: DomSceneResolver
  readonly #presentationHandle: number
  readonly #root: HTMLElement
  readonly #renderer: DomSvgRenderer
  readonly #cache: ByteBudgetLru<number, DisplayScene>
  readonly #prefetch: number
  readonly #imageResolver: DomImageResolver | undefined
  readonly #fontResolver: FontResolver | undefined
  readonly #hosts = new Map<number, HTMLDivElement>()
  #abort = new AbortController()
  #revision = 0
  #disposed = false

  constructor(
    resolver: DomSceneResolver,
    presentationHandle: number,
    root: HTMLElement,
    renderer = new DomSvgRenderer(),
    options: VirtualizedDomViewerOptions = {},
  ) {
    this.#resolver = resolver
    this.#presentationHandle = presentationHandle
    this.#root = root
    this.#renderer = renderer
    this.#cache = new ByteBudgetLru(options.sceneCacheBytes ?? 16 * 1024 * 1024)
    this.#prefetch = options.prefetchNeighbors ?? 1
    this.#imageResolver = options.imageResolver
    this.#fontResolver = options.fontResolver
  }

  get mountedSlideCount(): number {
    return this.#hosts.size
  }

  async setVisibleSlides(indices: readonly number[]): Promise<void> {
    if (this.#disposed) throw new Error('viewer is disposed')
    const visible = new Set(indices)
    for (const [index, host] of this.#hosts) {
      if (!visible.has(index)) {
        this.#renderer.clear(host)
        host.remove()
        this.#hosts.delete(index)
      }
    }
    this.#abort.abort()
    this.#abort = new AbortController()
    const signal = this.#abort.signal
    const revision = ++this.#revision
    await Promise.all(
      indices.map(async (index) => {
        const scene = await this.#scene(index, signal)
        if (signal.aborted || revision !== this.#revision) return
        let host = this.#hosts.get(index)
        if (host === undefined) {
          host = document.createElement('div')
          host.dataset['slideHost'] = String(index)
          this.#root.append(host)
          this.#hosts.set(index, host)
        }
        await this.#renderer.render(scene, host, {
          revision,
          slideIndex: index,
          signal,
          imageResolver: this.#imageResolver,
          fontResolver: this.#fontResolver,
        })
      }),
    )
    const neighbors = new Set<number>()
    for (const index of indices) {
      for (let distance = 1; distance <= this.#prefetch; distance += 1) {
        if (index >= distance) neighbors.add(index - distance)
        neighbors.add(index + distance)
      }
    }
    for (const index of visible) neighbors.delete(index)
    void Promise.all([...neighbors].map((index) => this.#scene(index, signal).catch(() => undefined)))
  }

  dispose(): void {
    if (this.#disposed) return
    this.#disposed = true
    this.#abort.abort()
    for (const host of this.#hosts.values()) {
      this.#renderer.clear(host)
      host.remove()
    }
    this.#hosts.clear()
    this.#cache.clear()
  }

  async #scene(index: number, signal: AbortSignal): Promise<DisplayScene> {
    const cached = this.#cache.get(index)
    if (cached !== undefined) return cached
    const bytes = await this.#resolver.resolveSlide(this.#presentationHandle, index, { signal })
    throwIfAborted(signal)
    const scene = decodeDisplayList(bytes)
    this.#cache.set(index, scene, scene.byteLength)
    return scene
  }
}

function removeMissing<ElementType extends Element>(
  elements: Map<string, ElementType>,
  retained: ReadonlySet<string>,
): void {
  for (const [id, element] of elements) {
    if (!retained.has(id)) {
      element.remove()
      elements.delete(id)
    }
  }
}

function semanticKey(semantic: SceneSemanticElement): string {
  return semantic.source?.semanticId ?? `${semantic.zOrder}:${semantic.shapeId}`
}

function presetPath(geometry: number, width: number, height: number): string {
  return presetGeometrySvgPath(presetGeometryPath(geometry, width, height))
}

function customPathData(path: readonly ScenePathCommand[]): string {
  const output: string[] = []
  let currentX = 0
  let currentY = 0
  for (const part of path) {
    if (part.kind === 'close') {
      output.push('Z')
    } else if (part.kind === 'move-to' || part.kind === 'line-to') {
      output.push(`${part.kind === 'move-to' ? 'M' : 'L'} ${part.x} ${part.y}`)
      currentX = part.x
      currentY = part.y
    } else if (part.kind === 'quadratic-to') {
      output.push(`Q ${part.controlX} ${part.controlY} ${part.x} ${part.y}`)
      currentX = part.x
      currentY = part.y
    } else if (part.kind === 'cubic-to') {
      output.push(`C ${part.control1X} ${part.control1Y} ${part.control2X} ${part.control2Y} ${part.x} ${part.y}`)
      currentX = part.x
      currentY = part.y
    } else {
      const start = part.startAngle / 60_000 * Math.PI / 180
      const sweep = part.sweepAngle / 60_000 * Math.PI / 180
      const centerX = currentX - part.widthRadius * Math.cos(start)
      const centerY = currentY - part.heightRadius * Math.sin(start)
      currentX = centerX + part.widthRadius * Math.cos(start + sweep)
      currentY = centerY + part.heightRadius * Math.sin(start + sweep)
      output.push(`A ${part.widthRadius} ${part.heightRadius} 0 ${Math.abs(sweep) > Math.PI ? 1 : 0} ${sweep >= 0 ? 1 : 0} ${currentX} ${currentY}`)
    }
  }
  return output.join(' ')
}

function resizedTransform(transform: SceneTransform, bounds: EmuRect | undefined): SceneTransform {
  return bounds === undefined ? transform : { ...transform, bounds }
}

function cssColor(color: { red: number; green: number; blue: number; alpha: number }): string {
  return `rgba(${color.red}, ${color.green}, ${color.blue}, ${color.alpha / 255})`
}

function dashPattern(dash: string | undefined, width: number): string | undefined {
  if (dash === 'dash') return `${4 * width} ${3 * width}`
  if (dash === 'dot') return `${width} ${2 * width}`
  if (dash === 'dashDot') return `${4 * width} ${2 * width} ${width} ${2 * width}`
  return undefined
}

function safeHyperlink(hyperlink: string | undefined): string | undefined {
  if (hyperlink === undefined) return undefined
  try {
    const url = new URL(hyperlink, document.baseURI)
    if (url.protocol === 'http:' || url.protocol === 'https:' || url.protocol === 'mailto:' || url.protocol === 'tel:') {
      return url.href
    }
  } catch {
    return undefined
  }
  return undefined
}

function required<Value>(values: readonly Value[], index: number): Value {
  const value = values[index]
  if (value === undefined) throw new Error('display list resource reference is out of bounds')
  return value
}

function setAttributes(element: Element, values: Readonly<Record<string, unknown>>): void {
  for (const [name, value] of Object.entries(values)) {
    if (value !== undefined) element.setAttribute(name, String(value))
  }
}

function throwIfAborted(signal: AbortSignal): void {
  if (signal.aborted) throw new DOMException('DOM/SVG rendering was cancelled', 'AbortError')
}
