import { lineBreakTokens, type FontLoadingHost } from '../canvas.js'
import { defaultFontFamily, detectFontScript, resolveAvailableFont } from '../font-resolution.js'

/** Private synchronous Wasm callback. Font sizes are hundredths of a point; widths are EMU. */
export type InlineTextMeasure = (text: string, size: number, family: string | undefined,
  bold: boolean, italic: boolean) => number | undefined

export function createInlineTextMeasurer(
  context: Pick<OffscreenCanvasRenderingContext2D, 'font' | 'measureText'>,
  host: FontLoadingHost,
): InlineTextMeasure {
  return (text, size, family, bold, italic) => {
    const requested = family ?? defaultFontFamily(detectFontScript(text))
    const font = resolveAvailableFont(text, size / 100 * 96 / 72, requested, requested, 'sans-serif', { bold, italic }, host)
    context.font = font.css
    // Match the paint planner's token measurements, including CJK grapheme breaks and soft hyphens.
    const width = lineBreakTokens(text).reduce((total, token) =>
      total + context.measureText(token.replaceAll('\u00AD', '')).width, 0)
    // Reserve the whole measured advance when converting to integer EMU. Rounding down can
    // make a packed text box rewrap its last word or space in the floating-point paint plan.
    return Math.ceil(width * 9525)
  }
}
