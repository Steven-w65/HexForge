export type OffsetString = string
export type Endian = 'little' | 'big'
export type ColorTheme = 'dark' | 'light'
/** Display origin only; template loading and parsing remain separate workflows. */
export type TemplateSource = 'none' | 'file' | 'draft'
export type FieldType = 'u8' | 'u16' | 'u32' | 'i8' | 'i16' | 'i32' | 'f32' | 'f64' | 'string' | 'bytes'

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

/** UI-owned request identity paired with a backend page response. */
export interface ViewportPage extends PageResponse {
  generation: number
}

export interface PageRequest {
  offset: bigint
  length: number
  generation: number
}

export interface TemplateField {
  name: string
  offset: OffsetString
  type: FieldType
  length?: number
  endianness?: Endian
  comment: string
}

export interface TemplateDefinition {
  version: 1
  name: string
  defaultEndianness: Endian
  fields: TemplateField[]
}

export interface ParsedField extends TemplateField {
  length: number
  endianness: Endian
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
