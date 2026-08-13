export const EMU_PER_CSS_PIXEL = 9_525

export interface GeometryTransform {
  readonly bounds: { readonly x: number; readonly y: number; readonly width: number; readonly height: number }
  readonly rotation: number
  readonly flipHorizontal: boolean
  readonly flipVertical: boolean
}

export interface GeometryGroupTransform {
  readonly outer: GeometryTransform
  readonly childX: number
  readonly childY: number
  readonly childWidth: number
  readonly childHeight: number
}

export type PresetPathCommand =
  | { readonly kind: 'move-to'; readonly x: number; readonly y: number }
  | { readonly kind: 'line-to'; readonly x: number; readonly y: number }
  | { readonly kind: 'ellipse'; readonly centerX: number; readonly centerY: number; readonly radiusX: number; readonly radiusY: number }
  | { readonly kind: 'round-rect'; readonly x: number; readonly y: number; readonly width: number; readonly height: number; readonly radius: number }
  | { readonly kind: 'rect'; readonly x: number; readonly y: number; readonly width: number; readonly height: number }
  | { readonly kind: 'close' }

export interface Matrix {
  readonly a: number
  readonly b: number
  readonly c: number
  readonly d: number
  readonly e: number
  readonly f: number
}

export interface PresetPathSink {
  moveTo(x: number, y: number): void
  lineTo(x: number, y: number): void
  ellipse(
    centerX: number,
    centerY: number,
    radiusX: number,
    radiusY: number,
    rotation: number,
    startAngle: number,
    endAngle: number,
  ): void
  roundRect(x: number, y: number, width: number, height: number, radius: number): void
  rect(x: number, y: number, width: number, height: number): void
  closePath(): void
}

const move = (x: number, y: number): PresetPathCommand => ({ kind: 'move-to', x, y })
const line = (x: number, y: number): PresetPathCommand => ({ kind: 'line-to', x, y })
const close: PresetPathCommand = { kind: 'close' }

/** Resolve a WPDL preset into one backend-neutral path plan. */
export function presetGeometryPath(
  geometry: number,
  width: number,
  height: number,
): readonly PresetPathCommand[] {
  if (geometry === 3) {
    return [{
      kind: 'ellipse',
      centerX: width / 2,
      centerY: height / 2,
      radiusX: Math.abs(width / 2),
      radiusY: Math.abs(height / 2),
    }]
  }
  if (geometry === 4) return [move(0, 0), line(width, height)]
  if (geometry === 5) return [move(width / 2, 0), line(width, height), line(0, height), close]
  if (geometry === 6) return [move(0, 0), line(width, height), line(0, height), close]
  if (geometry === 7) {
    return [move(width / 2, 0), line(width, height / 2), line(width / 2, height), line(0, height / 2), close]
  }
  if (geometry === 8) {
    return [move(width / 4, 0), line(width, 0), line(width * 3 / 4, height), line(0, height), close]
  }
  if (geometry === 9) {
    return [
      move(width / 4, 0), line(width * 3 / 4, 0), line(width, height / 2),
      line(width * 3 / 4, height), line(width / 4, height), line(0, height / 2), close,
    ]
  }
  if (geometry === 10 || geometry === 11 || geometry === 12) {
    const count = geometry === 10 ? 5 : geometry === 11 ? 8 : 10
    const points = Array.from({ length: count }, (_, index) => {
      const angle = -Math.PI / 2 + index * Math.PI * 2 / count
      const radius = geometry === 12 && index % 2 === 1 ? 0.22 : 0.5
      return {
        x: width / 2 + Math.cos(angle) * width * radius,
        y: height / 2 + Math.sin(angle) * height * radius,
      }
    })
    return points.map((point, index) => index === 0
      ? move(point.x, point.y)
      : line(point.x, point.y)).concat(close)
  }
  if (geometry === 13) {
    return [
      move(width * 0.35, 0), line(width * 0.65, 0), line(width * 0.65, height * 0.35),
      line(width, height * 0.35), line(width, height * 0.65), line(width * 0.65, height * 0.65),
      line(width * 0.65, height), line(width * 0.35, height), line(width * 0.35, height * 0.65),
      line(0, height * 0.65), line(0, height * 0.35), line(width * 0.35, height * 0.35), close,
    ]
  }
  if (geometry === 14) {
    return [move(0, 0), line(width * 0.65, 0), line(width, height / 2), line(width * 0.65, height), line(0, height), line(width * 0.35, height / 2), close]
  }
  if (geometry === 15) {
    return [move(0, height * 0.3), line(width * 0.6, height * 0.3), line(width * 0.6, 0), line(width, height / 2), line(width * 0.6, height), line(width * 0.6, height * 0.7), line(0, height * 0.7), close]
  }
  if (geometry === 16) {
    return [move(width, height * 0.3), line(width * 0.4, height * 0.3), line(width * 0.4, 0), line(0, height / 2), line(width * 0.4, height), line(width * 0.4, height * 0.7), line(width, height * 0.7), close]
  }
  if (geometry === 17) {
    return [move(width * 0.3, height), line(width * 0.3, height * 0.4), line(0, height * 0.4), line(width / 2, 0), line(width, height * 0.4), line(width * 0.7, height * 0.4), line(width * 0.7, height), close]
  }
  if (geometry === 18) {
    return [move(width * 0.3, 0), line(width * 0.3, height * 0.6), line(0, height * 0.6), line(width / 2, height), line(width, height * 0.6), line(width * 0.7, height * 0.6), line(width * 0.7, 0), close]
  }
  if (geometry === 19) {
    return [move(width * 0.2, 0), line(width * 0.8, 0), line(width, height), line(0, height), close]
  }
  if (geometry === 2) {
    return [{
      kind: 'round-rect', x: 0, y: 0, width, height,
      radius: Math.min(Math.abs(width), Math.abs(height)) / 8,
    }]
  }
  return [{ kind: 'rect', x: 0, y: 0, width, height }]
}

/** Project a shared preset plan to SVG path data without changing its geometry. */
export function presetGeometrySvgPath(commands: readonly PresetPathCommand[]): string {
  const output: string[] = []
  for (const command of commands) {
    if (command.kind === 'move-to') output.push(`M ${command.x} ${command.y}`)
    else if (command.kind === 'line-to') output.push(`L ${command.x} ${command.y}`)
    else if (command.kind === 'close') output.push('Z')
    else if (command.kind === 'rect') {
      output.push(`M ${command.x} ${command.y} H ${command.x + command.width} V ${command.y + command.height} H ${command.x} Z`)
    } else if (command.kind === 'round-rect') {
      const { x, y, width, height, radius } = command
      output.push(`M ${x + radius} ${y} H ${x + width - radius} Q ${x + width} ${y} ${x + width} ${y + radius} V ${y + height - radius} Q ${x + width} ${y + height} ${x + width - radius} ${y + height} H ${x + radius} Q ${x} ${y + height} ${x} ${y + height - radius} V ${y + radius} Q ${x} ${y} ${x + radius} ${y} Z`)
    } else {
      const { centerX, centerY, radiusX, radiusY } = command
      output.push(`M ${centerX - radiusX} ${centerY} A ${radiusX} ${radiusY} 0 1 0 ${centerX + radiusX} ${centerY} A ${radiusX} ${radiusY} 0 1 0 ${centerX - radiusX} ${centerY} Z`)
    }
  }
  return output.join(' ')
}

/** Project the same shared preset plan through the Canvas path API. */
export function projectPresetGeometryToCanvas(
  commands: readonly PresetPathCommand[],
  sink: PresetPathSink,
): void {
  for (const command of commands) {
    if (command.kind === 'move-to') sink.moveTo(command.x, command.y)
    else if (command.kind === 'line-to') sink.lineTo(command.x, command.y)
    else if (command.kind === 'close') sink.closePath()
    else if (command.kind === 'ellipse') {
      sink.ellipse(
        command.centerX,
        command.centerY,
        command.radiusX,
        command.radiusY,
        0,
        0,
        Math.PI * 2,
      )
    } else if (command.kind === 'round-rect') {
      sink.roundRect(command.x, command.y, command.width, command.height, command.radius)
    } else sink.rect(command.x, command.y, command.width, command.height)
  }
}

export function identityMatrix(): Matrix {
  return { a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 }
}

export function translation(x: number, y: number): Matrix {
  return { a: 1, b: 0, c: 0, d: 1, e: x, f: y }
}

function scale(x: number, y: number): Matrix {
  return { a: x, b: 0, c: 0, d: y, e: 0, f: 0 }
}

function rotation(radians: number): Matrix {
  const cosine = Math.cos(radians)
  const sine = Math.sin(radians)
  return { a: cosine, b: sine, c: -sine, d: cosine, e: 0, f: 0 }
}

export function multiplyMatrices(left: Matrix, right: Matrix): Matrix {
  return {
    a: left.a * right.a + left.c * right.b,
    b: left.b * right.a + left.d * right.b,
    c: left.a * right.c + left.c * right.d,
    d: left.b * right.c + left.d * right.d,
    e: left.a * right.e + left.c * right.f + left.e,
    f: left.b * right.e + left.d * right.f + left.f,
  }
}

export function groupTransformMatrix(group: GeometryGroupTransform): Matrix {
  const bounds = group.outer.bounds
  return [
    translation(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2),
    rotation(group.outer.rotation / 60_000 * Math.PI / 180),
    scale(group.outer.flipHorizontal ? -1 : 1, group.outer.flipVertical ? -1 : 1),
    translation(-bounds.width / 2, -bounds.height / 2),
    scale(
      group.childWidth === 0 ? 1 : bounds.width / group.childWidth,
      group.childHeight === 0 ? 1 : bounds.height / group.childHeight,
    ),
    translation(-group.childX, -group.childY),
  ].reduce(multiplyMatrices)
}

export function shapeTransformMatrix(transform: GeometryTransform): Matrix {
  const bounds = transform.bounds
  return [
    translation(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2),
    rotation(transform.rotation / 60_000 * Math.PI / 180),
    scale(transform.flipHorizontal ? -1 : 1, transform.flipVertical ? -1 : 1),
    translation(-bounds.width / 2, -bounds.height / 2),
  ].reduce(multiplyMatrices)
}

export function shapeSvgTransform(transform: GeometryTransform): string {
  const bounds = transform.bounds
  return [
    `translate(${bounds.x + bounds.width / 2} ${bounds.y + bounds.height / 2})`,
    `rotate(${transform.rotation / 60_000})`,
    `scale(${transform.flipHorizontal ? -1 : 1} ${transform.flipVertical ? -1 : 1})`,
    `translate(${-bounds.width / 2} ${-bounds.height / 2})`,
  ].join(' ')
}

export function groupSvgTransform(group: GeometryGroupTransform): string {
  const bounds = group.outer.bounds
  return [
    `translate(${bounds.x + bounds.width / 2} ${bounds.y + bounds.height / 2})`,
    `rotate(${group.outer.rotation / 60_000})`,
    `scale(${group.outer.flipHorizontal ? -1 : 1} ${group.outer.flipVertical ? -1 : 1})`,
    `translate(${-bounds.width / 2} ${-bounds.height / 2})`,
    `scale(${group.childWidth === 0 ? 1 : bounds.width / group.childWidth} ${group.childHeight === 0 ? 1 : bounds.height / group.childHeight})`,
    `translate(${-group.childX} ${-group.childY})`,
  ].join(' ')
}

export function toCssPixels(matrix: Matrix): Matrix {
  return { ...matrix, e: matrix.e / EMU_PER_CSS_PIXEL, f: matrix.f / EMU_PER_CSS_PIXEL }
}

export function cssMatrix(matrix: Matrix): string {
  return `matrix(${matrix.a}, ${matrix.b}, ${matrix.c}, ${matrix.d}, ${matrix.e}, ${matrix.f})`
}
