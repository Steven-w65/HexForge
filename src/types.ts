export type OffsetString = string
export type Endian = 'little' | 'big'
export type ColorTheme = 'dark' | 'light'
/** Display origin only; template loading and parsing remain separate workflows. */
export type TemplateSource = 'none' | 'file' | 'draft'
export type FieldType = 'u8' | 'u16' | 'u32' | 'u64' | 'i8' | 'i16' | 'i32' | 'i64' | 'f32' | 'f64' | 'bool' | 'string' | 'bytes' | 'struct' | 'array'

export interface FileInfo {
  name: string
  path: string
  size: OffsetString
  revision: string
  dirty: boolean
}

export interface PageResponse {
  offset: OffsetString
  bytes: number[]
  modifiedOffsets: OffsetString[]
  revision: string
}

export interface MinimapSampleRow {
  row: OffsetString
  bytes: number[]
  modifiedOffsets: OffsetString[]
}

export interface MinimapSamplesResponse {
  revision: OffsetString
  samples: MinimapSampleRow[]
}

/** Fixed-size edit summary for the file overview; never contains source bytes. */
export interface ModifiedOverview {
  binCount: number
  bins: number[]
}

/** UI-owned request identity paired with a backend page response. */
export interface ViewportPage extends PageResponse {
  generation: number
}

export interface PageRequest {
  offset: bigint
  length: number
  generation: number
}

export type TextEncoding = 'ascii' | 'utf8' | 'utf16le' | 'utf16be'
export type PlacementMode = 'absolute' | 'relative' | 'sequential'
export interface OffsetReference { ref: string; add?: OffsetString }
export interface LengthReference { ref: string; max: number }
export type LengthSpec = number | LengthReference
export interface Placement { mode: PlacementMode; offset?: OffsetString | OffsetReference }
export interface CountSpec { fixed?: number; ref?: string }
export type ConditionValue =
  | { type: 'unsigned' | 'signed'; value: string }
  | { type: 'bool'; value: boolean }
  | { type: 'string'; value: string }
export interface Condition { ref: string; op: 'eq' | 'ne' | 'lt' | 'lte' | 'gt' | 'gte'; value: ConditionValue }
export interface BitFlag { bit: number; name: string }
export type ExpectedValue = ConditionValue | { type: 'float' | 'bytes'; value: string }
export type Expectation =
  | { kind: 'equals'; value: ExpectedValue }
  | { kind: 'oneOf'; values: ExpectedValue[] }
  | { kind: 'range'; min: ExpectedValue; max: ExpectedValue }
/** A root/struct field has a name; an array element is deliberately unnamed. */
export interface TemplateField {
  name?: string
  type: FieldType
  placement?: Placement
  align?: number
  endianness?: Endian
  comment?: string
  length?: LengthSpec
  maxLength?: number
  encoding?: TextEncoding
  fields?: TemplateField[]
  element?: TemplateField
  count?: CountSpec
  condition?: Condition
  enumLabels?: Record<string, string>
  bitFlags?: BitFlag[]
  expect?: Expectation
}
export interface TemplateDefinition {
  name: string
  defaultEndianness: Endian
  fields: TemplateField[]
}
export interface ParseDiagnostic { code: string; message: string }
export interface ParsedNode {
  kind: 'leaf' | 'struct' | 'array' | 'error'
  name: string
  path: string
  type: FieldType
  offset: OffsetString | null
  length: OffsetString | null
  value: string | null
  endianness: Endian | null
  comment: string
  enumLabel: string | null
  flags: string[]
  diagnostics: ParseDiagnostic[]
  children: ParsedNode[]
}
export type ParsedResult = ParsedNode
/** Small, fully decoded leaf summary consumed by minimap and navigation. */
export interface NavigableParsedLeaf {
  name: string
  path: string
  offset: OffsetString
  length: number
  type: string
  value: string
}

export interface AppError {
  code: string
  message: string
  detail?: string | null
}

export interface DirtyState {
  dirty: boolean
  revision: string
}

export interface UndoResponse extends DirtyState {
  undone: boolean
  offset?: OffsetString
}

export interface SaveResponse extends DirtyState {
  bytesWritten: OffsetString
  destination: string
  file: FileInfo
}

export interface SearchResponse {
  matches: OffsetString[]
  truncated: boolean
}

export interface OperationProgress {
  operationId: string
  phase: 'search' | 'parse' | 'save' | 'csv' | 'complete'
  processed: OffsetString
  total: OffsetString
}
