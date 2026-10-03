import JSZip from 'jszip'
import pptxgen from 'pptxgenjs'
import { textSlide, textWorkload } from '../text-workload.mjs'

export async function authorTextDeck(bindingTokens = false, inputs = Array.from({ length: textWorkload.slides }, (_, index) => textSlide(index))) {
  const deck = new pptxgen()
  deck.layout = 'LAYOUT_WIDE'
  deck.author = 'wasmppt equivalent-output benchmark'
  deck.subject = textWorkload.id
  for (let index = 0; index < textWorkload.slides; index += 1) {
    const slide = deck.addSlide()
    slide.background = { color: textWorkload.background }
    for (const field of inputs[index]) {
      slide.addText(bindingTokens ? `{{${field.binding}}}` : field.text, { ...field.options })
    }
  }
  return new Uint8Array(await deck.write({ outputType: 'arraybuffer', compression: textWorkload.compression }))
}

/** Create a valid POTX off the clock; only its main content type differs from the authored PPTX. */
export async function createTextTemplate() {
  const zip = await JSZip.loadAsync(await canonicalizeNotesMasterOrder(await authorTextDeck(true)))
  const types = await zip.file('[Content_Types].xml').async('string')
  const presentationType = 'application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml'
  if (!types.includes(presentationType)) throw new Error('authored template has no presentation main content type')
  zip.file('[Content_Types].xml', types.replace(presentationType, 'application/vnd.openxmlformats-officedocument.presentationml.template.main+xml'))
  return zip.generateAsync({ type: 'uint8array', compression: 'DEFLATE' })
}

/** Explicit measured adapter policy for PptxGenJS 4.0.1's non-schema-order notes master list. */
export async function canonicalizeNotesMasterOrder(input) {
  const zip = await JSZip.loadAsync(input)
  const xml = await zip.file('ppt/presentation.xml').async('string')
  const notes = xml.match(/<p:notesMasterIdLst>[\s\S]*?<\/p:notesMasterIdLst>/g)
  const slideLists = xml.match(/<p:sldIdLst>[\s\S]*?<\/p:sldIdLst>/g)
  if (notes?.length !== 1 || slideLists?.length !== 1) throw new Error('unexpected PptxGenJS presentation topology')
  if (xml.indexOf(notes[0]) < xml.indexOf(slideLists[0])) return input
  zip.file('ppt/presentation.xml', xml.replace(notes[0], '').replace(slideLists[0], notes[0] + slideLists[0]))
  return zip.generateAsync({ type: 'uint8array', compression: 'DEFLATE' })
}
