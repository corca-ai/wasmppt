/** Versioned equivalent-output contract; this is separate from the native budget fixtures. */
export const textWorkload = Object.freeze({
  id: 'generated-text-10-v1',
  slides: 10,
  fieldsPerSlide: 8,
  sizeEmu: [12192000, 6858000],
  fontFace: 'Arial',
  fontSize: 10,
  color: '000000',
  background: 'FFFFFF',
  compression: true,
  textRepeats: 8,
})

export function textSlide(slideIndex) {
  return Array.from({ length: textWorkload.fieldsPerSlide }, (_, field) => ({
    binding: `text_${slideIndex}_${field}`,
    text: `Slide ${slideIndex} field ${field}: 한국어 العربية 👨🏽‍💻 & <benchmark> ${'benchmark payload '.repeat(textWorkload.textRepeats)}`,
    options: {
      x: 0.5, y: 0.3 + field * 0.65, w: 11.5, h: 0.5,
      fontFace: textWorkload.fontFace,
      fontSize: textWorkload.fontSize,
      color: textWorkload.color,
      margin: 0,
      breakLine: false,
      valign: 'top',
    },
  }))
}

/** Check rendered text commands, rather than accepting unused strings elsewhere in the package. */
export function textSlideFailures(scene, slideIndex) {
  const failures = []
  if (scene.width !== textWorkload.sizeEmu[0] || scene.height !== textWorkload.sizeEmu[1]) failures.push('page dimensions changed')
  if (scene.diagnostics.length > 0) failures.push('unexpected resolver diagnostics')
  const blocks = scene.commands.flatMap((command) => {
    if (command.kind === 'draw-text') return [{ text: scene.strings[command.text], bounds: command.bounds, styles: [command.style] }]
    if (command.kind !== 'draw-rich-text') return []
    return [{
      text: command.frame.paragraphs.map((paragraph) => paragraph.runs.map((run) => run.text).join('')).join('\n'),
      bounds: command.bounds,
      styles: command.frame.paragraphs.flatMap((paragraph) => paragraph.runs.filter((run) => run.text.length > 0).map((run) => run.style)),
    }]
  }).filter((block) => block.text.length > 0)
  const expected = textSlide(slideIndex)
  if (blocks.length !== expected.length) failures.push('requested text box count changed')
  const background = scene.commands.find((command) => command.kind === 'clear')?.color
  if (!background || background.red !== 255 || background.green !== 255 || background.blue !== 255 || background.alpha !== 255) failures.push('background changed')
  for (const [field, value] of expected.entries()) {
    const block = blocks[field]
    if (block?.text !== value.text) failures.push(`field ${field} text changed or moved`)
    const bounds = block?.bounds
    const expectedBounds = [value.options.x, value.options.y, value.options.w, value.options.h].map((inches) => Math.round(inches * 914400))
    if (!bounds || [bounds.x, bounds.y, bounds.width, bounds.height].some((axis, index) => axis !== expectedBounds[index])) failures.push(`field ${field} geometry changed`)
    if (!block?.styles.length || block.styles.some((style) => style.fontSize !== textWorkload.fontSize * 100 || style.fontFamily !== textWorkload.fontFace)) failures.push(`field ${field} font changed`)
    if (block?.styles.some((style) => style.color.red !== 0 || style.color.green !== 0 || style.color.blue !== 0 || style.color.alpha !== 255 || style.bold || style.italic)) failures.push(`field ${field} text paint changed`)
  }
  return failures
}
