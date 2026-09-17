export type OffsetString = string
export type Endian = 'little' | 'big'
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

export interface TemplateField {
  name: string
  offset: OffsetString
  type: FieldType
  length?: number
  endianness: Endian
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
  value: string
}

export interface AppError {
  code: string
  message: string
  detail?: string
}
