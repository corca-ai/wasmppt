import assert from 'node:assert/strict'
import test from 'node:test'

import {
  groupSvgTransform,
  groupTransformMatrix,
  presetGeometryPath,
  presetGeometrySvgPath,
  projectPresetGeometryToCanvas,
  shapeSvgTransform,
  shapeTransformMatrix,
} from '../dist/index.js'

test('Canvas and SVG projections consume one preset geometry plan', () => {
  for (let geometry = 1; geometry <= 19; geometry += 1) {
    const plan = presetGeometryPath(geometry, 120, 80)
    const canvas = []
    projectPresetGeometryToCanvas(plan, {
      moveTo: (x, y) => canvas.push(['M', x, y]),
      lineTo: (x, y) => canvas.push(['L', x, y]),
      ellipse: (...values) => canvas.push(['ellipse', ...values]),
      roundRect: (...values) => canvas.push(['roundRect', ...values]),
      rect: (...values) => canvas.push(['rect', ...values]),
      closePath: () => canvas.push(['Z']),
    })
    const projectedCommandCount = (presetGeometrySvgPath(plan).match(/\b(?:M|L|A|Q|H|V|Z)\b/g) ?? []).length
    assert(plan.length > 0, `geometry ${geometry}`)
    assert(canvas.length > 0, `Canvas geometry ${geometry}`)
    assert(projectedCommandCount > 0, `SVG geometry ${geometry}`)
  }
})

test('Canvas matrices and SVG transform lists retain the same shape semantics', () => {
  const shape = {
    bounds: { x: 100, y: 200, width: 400, height: 300 },
    rotation: 5_400_000,
    flipHorizontal: true,
    flipVertical: false,
  }
  const shapeMatrix = shapeTransformMatrix(shape)
  assert(Math.abs(shapeMatrix.a) < Number.EPSILON)
  assert(Math.abs(shapeMatrix.d) < Number.EPSILON)
  assert.deepEqual(
    { b: shapeMatrix.b, c: shapeMatrix.c, e: shapeMatrix.e, f: shapeMatrix.f },
    { b: -1, c: -1, e: 450, f: 550 },
  )
  assert.equal(
    shapeSvgTransform(shape),
    'translate(300 350) rotate(90) scale(-1 1) translate(-200 -150)',
  )

  const group = {
    outer: shape,
    childX: 10,
    childY: 20,
    childWidth: 200,
    childHeight: 100,
  }
  const matrix = groupTransformMatrix(group)
  assert(Number.isFinite(matrix.a) && Number.isFinite(matrix.f))
  assert.equal(
    groupSvgTransform(group),
    'translate(300 350) rotate(90) scale(-1 1) translate(-200 -150) scale(2 3) translate(-10 -20)',
  )
})
