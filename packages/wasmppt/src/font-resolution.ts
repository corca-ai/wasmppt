import type { FontLoadingHost, FontScript, ResolvedFont, ThemeFontSet } from './canvas.js'

export const DEFAULT_THEME_FONTS: ThemeFontSet = Object.freeze({
  latin: 'Arial', eastAsian: 'Noto Sans CJK KR', complexScript: 'Noto Sans Arabic',
})

export function defaultFontFamily(script: FontScript): string {
  return script === 'east-asian' ? DEFAULT_THEME_FONTS.eastAsian
    : script === 'complex' ? DEFAULT_THEME_FONTS.complexScript : DEFAULT_THEME_FONTS.latin
}

/** Shared available-font resolution, independent of web-font loading, caches and shaping. */
export function resolveAvailableFont(text: string, sizePixels: number, requested: string,
  family: string, fallbackFamily: string, emphasis: { readonly bold?: boolean; readonly italic?: boolean },
  host: Pick<FontLoadingHost, 'check'>): ResolvedFont {
  const script = detectFontScript(text)
  const prefix = `${emphasis.italic === true ? 'italic' : 'normal'} ${emphasis.bold === true ? '700' : '400'}`
  const css = `${prefix} ${sizePixels}px ${quoteFontFamily(family)}`
  const exact = host.check(css, representativeText(script, text))
  const resolvedFamily = exact ? family : fallbackFamily
  const cssFamily = exact
    ? `${quoteFontFamily(family)}, ${quoteFontFamily(fallbackFamily)}`
    : quoteFontFamily(fallbackFamily)
  return Object.freeze({
    requestedFamily: requested,
    family: resolvedFamily,
    script,
    exact,
    css: `${prefix} ${sizePixels}px ${cssFamily}`,
    sizePixels,
  })
}

export function detectFontScript(text: string): FontScript {
  if (/\p{Script=Hangul}|\p{Script=Han}|\p{Script=Hiragana}|\p{Script=Katakana}/u.test(text)) {
    return 'east-asian'
  }
  if (/\p{Script=Arabic}|\p{Script=Hebrew}|\p{Script=Devanagari}|\p{Script=Thai}|\p{Script=Lao}|\p{Script=Khmer}/u.test(text)) {
    return 'complex'
  }
  return 'latin'
}

function representativeText(script: FontScript, text: string): string {
  if (text !== '') return text
  if (script === 'east-asian') return '한국語'
  if (script === 'complex') return 'العربية'
  return 'Aa'
}

function quoteFontFamily(family: string): string {
  if (/^(serif|sans-serif|monospace|cursive|fantasy|system-ui)$/i.test(family)) return family
  return `"${family.replaceAll('"', '\\"')}"`
}
