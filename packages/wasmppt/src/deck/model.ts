/** Semantic authoring data. Keys identify content; source spans describe its current location. */
export type DeckSemanticRole =
  | 'title' | 'subtitle' | 'prose' | 'section' | 'list' | 'listItem'
  | 'figure' | 'caption' | 'gallery' | 'table' | 'chart' | 'code'
  | 'diagram' | 'displayMath' | 'quote' | 'credit' | 'definition'
  | 'definitionTerm' | 'definitionDescription' | 'statement'

export type DeckSplitPolicy = 'children' | 'code-lines' | 'list-items' | 'never' | 'table-rows' | 'text'

/** UTF-8 byte offsets in one host-owned source version. */
export interface DeckSourceRange {
  readonly source: string
  readonly start: number
  readonly end: number
  readonly version?: string
}

export interface DeckTextMarks {
  readonly bold?: boolean
  readonly italic?: boolean
  readonly strikethrough?: boolean
  readonly inlineCode?: boolean
}

export interface DeckHyperlink {
  readonly kind: 'email' | 'source-anchor' | 'telephone' | 'web'
  readonly target: string
}

export interface DeckRichTextRun {
  readonly text: string
  readonly marks?: DeckTextMarks
  readonly hyperlink?: DeckHyperlink | null
}

export interface DeckRichText { readonly runs: readonly DeckRichTextRun[] }

export interface DeckListItem {
  readonly key: string
  readonly source?: DeckSourceRange
  readonly blocks: readonly DeckSemanticNode[]
  readonly children: readonly DeckListContent[]
}

export interface DeckListContent {
  readonly items: readonly DeckListItem[]
  readonly ordered: boolean
  readonly start: number
}

export interface DeckTableColumn {
  readonly key: string
  readonly source?: DeckSourceRange
  readonly alignment: 'start' | 'center' | 'end'
}

export interface DeckTableCell {
  readonly key: string
  readonly source?: DeckSourceRange
  readonly content: DeckRichText
}

export interface DeckTableRow {
  readonly key: string
  readonly source?: DeckSourceRange
  readonly cells: readonly DeckTableCell[]
}

export interface DeckTableContent {
  readonly columns: readonly DeckTableColumn[]
  readonly headerRows: number
  readonly rows: readonly DeckTableRow[]
}

export interface DeckChartContent {
  readonly kind: 'bar' | 'column' | 'line' | 'area' | 'pie' | 'doughnut' | 'scatter'
  readonly categories: readonly string[]
  readonly series: readonly { readonly name: string; readonly values: readonly number[] }[]
}

export type DeckSemanticContent =
  | { readonly kind: 'text'; readonly text: DeckRichText }
  | { readonly kind: 'children'; readonly nodes: readonly DeckSemanticNode[] }
  | { readonly kind: 'image'; readonly assetId: string; readonly altText: string }
  | { readonly kind: 'svg'; readonly assetId: string; readonly fallbackAssetId?: string | null; readonly sourceText?: string | null }
  | { readonly kind: 'list'; readonly list: DeckListContent }
  | { readonly kind: 'table'; readonly table: DeckTableContent }
  | { readonly kind: 'chart'; readonly chart: DeckChartContent }
  | { readonly kind: 'code'; readonly code: string; readonly language?: string | null }

export interface DeckSemanticNode {
  readonly key: string
  readonly source?: DeckSourceRange
  readonly role: DeckSemanticRole
  readonly split?: DeckSplitPolicy
  readonly content: DeckSemanticContent
}

export type DeckMediaTextProximity = 'adjacent-blocks' | 'blank-separated-blocks' | 'same-paragraph'
export type DeckMediaTextSide = 'after-media' | 'before-media'

export interface DeckMediaTextRelation {
  readonly mediaNodeKey: string
  readonly textNodeKey: string
  readonly proximity: DeckMediaTextProximity
  readonly textSide: DeckMediaTextSide
  readonly explicitCaption: boolean
}

export interface DeckLogicalSlide {
  readonly key: string
  readonly source?: DeckSourceRange
  readonly kind: 'content' | 'title'
  readonly hidden?: boolean
  readonly nodes: readonly DeckSemanticNode[]
  readonly mediaTextRelations?: readonly DeckMediaTextRelation[]
}

export interface DeckSpec {
  readonly key: string
  readonly slides: readonly DeckLogicalSlide[]
}

export interface DeckAssetInput {
  readonly bytes: Uint8Array
  readonly mediaType: 'image/png' | 'image/jpeg' | 'image/gif' | 'image/svg+xml'
  readonly intrinsicSize?: { readonly width: number; readonly height: number }
}

export interface DeckFontInput {
  readonly family: string
  readonly bytes: Uint8Array
}

export interface DeckDiagnostic {
  readonly code: string
  readonly category: 'content' | 'template' | 'resource' | 'font' | 'layout' | 'engine'
  readonly severity: 'info' | 'warning' | 'error'
  readonly message: string
  readonly source?: DeckSourceRange
  readonly nodeKey?: string
  readonly pageId?: string
}

export type DeckResult<T> =
  | { readonly ok: true; readonly value: T; readonly diagnostics: readonly DeckDiagnostic[] }
  | { readonly ok: false; readonly diagnostics: readonly DeckDiagnostic[] }

export interface DeckTemplateDescription {
  readonly pageSize: { readonly width: number; readonly height: number }
  readonly layouts: readonly {
    readonly id: string
    readonly capability: 'title' | 'statement' | 'content-envelope'
    readonly matchingName: string
    readonly regions: readonly {
      readonly id: string
      readonly role: string
      readonly frame: { readonly x: number; readonly y: number; readonly width: number; readonly height: number }
      readonly accepts: readonly DeckSemanticRole[]
    }[]
  }[]
  readonly requiredFonts: readonly string[]
  readonly diagnostics: readonly DeckDiagnostic[]
}
