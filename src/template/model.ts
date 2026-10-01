import type { NavigableParsedLeaf, ParsedResult, TemplateDefinition, ConditionValue, ExpectedValue, FieldType, ParsedNode, TemplateField } from '../types'

export interface TemplateIssue { path: string; message: string }
const UNSIGNED = new Set<FieldType>(['u8', 'u16', 'u32', 'u64'])
const SIGNED = new Set<FieldType>(['i8', 'i16', 'i32', 'i64'])
const NUMERIC = new Set<FieldType>([...UNSIGNED, ...SIGNED, 'f32', 'f64'])
const U64_MAX = (1n << 64n) - 1n
const I64_MIN = -(1n << 63n)
const I64_MAX = (1n << 63n) - 1n
const MAX_READ = 1_048_576

export function newTemplate(): TemplateDefinition {
  return { name: 'Untitled', defaultEndianness: 'little', fields: [] }
}

export function newTemplateField(type: FieldType = 'u8', name?: string): TemplateField {
  const base: TemplateField = { ...(name === undefined ? {} : { name }), type }
  if (type === 'string') return { ...base, encoding: 'utf8', length: 1 }
  if (type === 'bytes') return { ...base, length: 1 }
  if (type === 'struct') return { ...base, fields: [] }
  if (type === 'array') return { ...base, count: { fixed: 1 }, element: { type: 'u8' } }
  return base
}

/** Containers/errors never masquerade as decoded leaves. Offsets remain strings. */
export function flattenResultLeaves(results: ParsedResult[]): NavigableParsedLeaf[] {
  const leaves: NavigableParsedLeaf[] = []
  const visit = (result: ParsedNode): void => {
    if (result.kind === 'leaf' && result.diagnostics.length === 0 && result.offset !== null && result.length !== null && result.value !== null) {
      const length = Number(result.length)
      if (Number.isSafeInteger(length) && length > 0) leaves.push({ name: result.name, path: result.path, offset: result.offset, length, type: result.type, value: result.value })
    }
    for (const child of result.children) visit(child)
  }
  results.forEach(visit)
  return leaves
}

export function hasResultDiagnostics(results: ParsedResult[]): boolean {
  return results.some(result => result.diagnostics.length > 0 || hasResultDiagnostics(result.children))
}

/** Insert a deep copy immediately after the source in the same named scope. */
export function duplicateTemplateField(fields: TemplateField[], index: number): TemplateField[] {
  const source = fields[index]
  if (!source?.name) return fields
  const names = new Set(fields.map(field => field.name))
  const base = `${source.name}Copy`
  let name = base
  let suffix = 2
  while (names.has(name)) name = `${base}${suffix++}`
  // Template definitions are JSON values; serialize to unwrap Vue proxies and
  // ensure nested child fields cannot mutate the source after duplication.
  const copy = JSON.parse(JSON.stringify(source)) as TemplateField
  copy.name = name
  return [...fields.slice(0, index + 1), copy, ...fields.slice(index + 1)]
}

function decimal(value: string | undefined, signed: boolean): boolean {
  if (value === undefined || !/^-?\d+$/.test(value) || (!signed && value.startsWith('-'))) return false
  try {
    const number = BigInt(value)
    return signed ? number >= I64_MIN && number <= I64_MAX : number >= 0n && number <= U64_MAX
  } catch { return false }
}

function conditionTypeMatches(type: FieldType, value: ConditionValue): boolean {
  return value.type === 'unsigned' ? UNSIGNED.has(type)
    : value.type === 'signed' ? SIGNED.has(type)
      : value.type === 'bool' ? type === 'bool' : type === 'string'
}

function integerWidth(type: FieldType): bigint {
  return BigInt(type.endsWith('64') ? 64 : type.endsWith('32') ? 32 : type.endsWith('16') ? 16 : 8)
}

function expectedValueValid(value: ExpectedValue, type: FieldType): boolean {
  if (value.type === 'unsigned' || value.type === 'signed') {
    if (!conditionTypeMatches(type, value) || !decimal(value.value, value.type === 'signed')) return false
    const bits = integerWidth(type), number = BigInt(value.value)
    return value.type === 'unsigned' ? number < (1n << bits) : number >= -(1n << (bits - 1n)) && number < (1n << (bits - 1n))
  }
  if (value.type === 'float') return ['f32', 'f64'].includes(type) && /^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/.test(value.value) && Number.isFinite(Number(value.value))
  if (value.type === 'bool') return type === 'bool'
  if (value.type === 'string') return type === 'string' && new TextEncoder().encode(value.value).length <= MAX_READ
  return type === 'bytes' && value.value.trim().split(/\s+/).filter(Boolean).every(part => /^[\da-fA-F]{2}$/.test(part)) && value.value.length <= MAX_READ * 3
}

function parentScope(path: string): string | null {
  if (path.endsWith(']')) {
    const open = path.lastIndexOf('[')
    return open >= 0 ? path.slice(0, open) : null
  }
  const dot = path.lastIndexOf('.')
  return dot >= 0 ? path.slice(0, dot) : null
}

/** Immediate editor feedback; Rust remains authoritative for save/apply and dynamic references. */
export function validateTemplate(template: TemplateDefinition): TemplateIssue[] {
  const issues: TemplateIssue[] = []
  const seen = new Map<string, FieldType>()
  let definitions = 0
  const add = (path: string, message: string): void => { issues.push({ path, message }) }
  if (!template.name.trim()) add('template', 'Template name must not be empty.')

  const checkRef = (path: string, ref: string, allowed: (type: FieldType) => boolean, label: string): void => {
    if (!ref.trim()) { add(path, `${label} needs a reference path.`); return }
    // Resolve nearest lexical scope first, matching the runtime's instantiated paths.
    let scope = parentScope(path)
    let type: FieldType | undefined
    while (scope !== null) {
      type = seen.get(`${scope}.${ref}`)
      if (type) break
      scope = parentScope(scope)
    }
    type ??= seen.get(ref)
    // A literal indexed reference needs runtime traversal to prove it exists.
    if (!type && ref.includes('[')) return
    if (!type) add(path, `${label} must reference an earlier field: ${ref}.`)
    else if (!allowed(type)) add(path, `${label} has the wrong reference type: ${ref}.`)
  }

  const checkFields = (fields: TemplateField[], scope: string, depth: number): void => {
    const names = new Set<string>()
    for (const field of fields) {
      const name = field.name ?? ''
      const path = scope ? `${scope}.${name}` : name
      if (!name.trim() || /[.\[\]]/.test(name)) add(path, 'Field name must be nonempty and cannot include ., [ or ].')
      if (names.has(name)) add(path, 'Field names must be unique in this structure.')
      names.add(name)
      checkField(field, path, depth, false)
      if (UNSIGNED.has(field.type) || SIGNED.has(field.type) || field.type === 'bool' || field.type === 'string') seen.set(path, field.type)
    }
  }

  const checkField = (field: TemplateField, path: string, depth: number, element: boolean): void => {
    definitions += 1
    if (definitions > 4096) add(path, 'Template exceeds 4096 definitions.')
    if (depth > 8) add(path, 'Template exceeds 8 nesting levels.')
    if (element && (field.name !== undefined || field.placement !== undefined || field.condition !== undefined)) add(path, 'Array elements must be unnamed, sequential and unconditional.')
    if (field.placement) {
      if (field.placement.mode === 'sequential' && field.placement.offset !== undefined) add(path, 'Sequential placement cannot specify offset.')
      if (field.placement.mode !== 'sequential') {
        const offset = field.placement.offset
        if (typeof offset === 'object') {
          checkRef(path, offset.ref, type => UNSIGNED.has(type), 'Offset')
          if (offset.add !== undefined && !decimal(offset.add, false)) add(path, 'Offset addition must be a u64 decimal string.')
        } else if (!decimal(offset, false)) add(path, 'Placement offset must be a u64 decimal string or unsigned reference.')
      }
    }
    if (field.align !== undefined && (!Number.isSafeInteger(field.align) || field.align < 1 || field.align > MAX_READ || (field.align & (field.align - 1)) !== 0)) add(path, 'Alignment must be a power of two up to 1048576.')
    if (field.condition) {
      checkRef(path, field.condition.ref, type => conditionTypeMatches(type, field.condition!.value), 'Condition')
      if ((field.condition.value.type === 'bool' || field.condition.value.type === 'string') && !['eq', 'ne'].includes(field.condition.op)) add(path, 'Boolean/text conditions support eq and ne only.')
      if ((field.condition.value.type === 'unsigned' || field.condition.value.type === 'signed') && !decimal(field.condition.value.value, field.condition.value.type === 'signed')) add(path, 'Condition integer must fit its type.')
    }
    if (field.enumLabels && !UNSIGNED.has(field.type) && !SIGNED.has(field.type)) add(path, 'Enum labels require an integer field.')
    if (field.bitFlags && !UNSIGNED.has(field.type)) add(path, 'Bit flags require an unsigned integer field.')
    if (field.bitFlags) {
      const bits = new Set<number>()
      const flagNames = new Set<string>()
      for (const flag of field.bitFlags) {
        if (!Number.isSafeInteger(flag.bit) || flag.bit < 0 || flag.bit >= Number(integerWidth(field.type)) || bits.has(flag.bit) || !flag.name.trim() || flagNames.has(flag.name)) add(path, 'Bit flags need unique in-range bits and names.')
        bits.add(flag.bit)
        flagNames.add(flag.name)
      }
    }
    if (field.enumLabels) for (const [key, label] of Object.entries(field.enumLabels)) {
      let inWidth = false
      if (decimal(key, SIGNED.has(field.type)) && (SIGNED.has(field.type) || UNSIGNED.has(field.type))) {
        const number = BigInt(key)
        const bits = integerWidth(field.type)
        inWidth = SIGNED.has(field.type)
          ? number >= -(1n << (bits - 1n)) && number < (1n << (bits - 1n))
          : number < (1n << bits)
      }
      if (!inWidth || !label.trim()) add(path, 'Enum keys must fit the integer field width and labels cannot be blank.')
    }

    if (typeof field.length === 'object') checkRef(path, field.length.ref, type => UNSIGNED.has(type), 'Length')
    if (field.expect) {
      const values = field.expect.kind === 'equals' ? [field.expect.value] : field.expect.kind === 'oneOf' ? field.expect.values : [field.expect.min, field.expect.max]
      if (values.length === 0 || values.length > 256 || values.some(value => !expectedValueValid(value, field.type))) add(path, 'Expected values must match the field type and width; oneOf needs 1 to 256 values.')
      else if (field.expect.kind === 'range') {
        if (!NUMERIC.has(field.type)) add(path, 'Expected ranges require numeric fields.')
        else {
          const min = field.expect.min.value, max = field.expect.max.value
          const inverted = ['f32', 'f64'].includes(field.type) ? Number(min) > Number(max) : BigInt(String(min)) > BigInt(String(max))
          if (inverted) add(path, 'Expected range minimum exceeds maximum.')
        }
      }
    }
    const bound = typeof field.length === 'object' ? field.length.max : field.length ?? field.maxLength
    const validBound = Number.isSafeInteger(bound) && bound! >= 1 && bound! <= MAX_READ
    if (NUMERIC.has(field.type) || field.type === 'bool') {
      if (field.length !== undefined || field.maxLength !== undefined || field.encoding !== undefined || field.fields !== undefined || field.element !== undefined || field.count !== undefined) add(path, 'Scalar fields cannot have text/byte/container properties.')
      if (field.type === 'bool' && (field.endianness !== undefined || field.enumLabels !== undefined || field.bitFlags !== undefined)) add(path, 'Bool cannot have endian, enum or flag properties.')
    } else if (field.type === 'bytes') {
      if (!validBound || field.length === undefined || field.endianness !== undefined || field.maxLength !== undefined || field.encoding !== undefined || field.fields !== undefined || field.element !== undefined || field.count !== undefined) add(path, 'Bytes require only a length from 1 to 1048576.')
    } else if (field.type === 'string') {
      if (!validBound || (field.length === undefined) === (field.maxLength === undefined) || field.encoding === undefined || field.endianness !== undefined || field.fields !== undefined || field.element !== undefined || field.count !== undefined) add(path, 'Text needs encoding and exactly one bounded length.')
      if ((field.encoding === 'utf16le' || field.encoding === 'utf16be') && bound !== undefined && bound % 2 !== 0) add(path, 'UTF-16 bounds must contain whole code units.')
    } else if (field.type === 'struct') {
      if (!field.fields || field.length !== undefined || field.maxLength !== undefined || field.encoding !== undefined || field.element !== undefined || field.count !== undefined) add(path, 'Struct needs a fields list.')
      else checkFields(field.fields, path, depth + 1)
    } else if (field.type === 'array') {
      if (!field.count || !field.element || field.length !== undefined || field.maxLength !== undefined || field.encoding !== undefined || field.fields !== undefined) add(path, 'Array needs count and element.')
      if (field.count) {
        if ((field.count.fixed === undefined) === (field.count.ref === undefined)) add(path, 'Array count must have exactly one fixed or ref value.')
        if (field.count.fixed !== undefined && (!Number.isSafeInteger(field.count.fixed) || field.count.fixed < 0 || field.count.fixed > 10_000)) add(path, 'Fixed array count exceeds the result limit.')
        if (field.count.ref !== undefined) checkRef(path, field.count.ref, type => UNSIGNED.has(type), 'Array count')
      }
      if (field.element) checkField(field.element, `${path}[]`, depth + 1, true)
    }
  }

  checkFields(template.fields, '', 1)
  // Count containers as well as leaves. BigInt prevents nested products from
  // rounding before the budget check. Dynamic counts reserve one element here;
  // Rust enforces their actual expansion when the count is decoded.
  const bytesOf = (value: string): bigint => BigInt(new TextEncoder().encode(value).length)
  const budget = (field: TemplateField, depth: number, path: string): { nodes: bigint; decoded: bigint } => {
    if (depth > 8) return { nodes: 0n, decoded: 0n }
    let nodes = 1n
    const labelBytes = Object.values(field.enumLabels ?? {}).reduce((maximum, label) => { const size = bytesOf(label); return size > maximum ? size : maximum }, 0n)
    const flagBytes = (field.bitFlags ?? []).reduce((sum, flag) => sum + bytesOf(flag.name), 0n)
    let decoded = 128n + bytesOf(path) * 2n + bytesOf(field.comment ?? '') + labelBytes + flagBytes
    if (field.type === 'struct') {
      for (const child of field.fields ?? []) {
        const item = budget(child, depth + 1, `${path}.${child.name ?? ''}`)
        nodes += item.nodes; decoded += item.decoded
      }
    } else if (field.type === 'array' && field.element) {
      const item = budget(field.element, depth + 1, `${path}[]`)
      const fixed = field.count?.fixed ?? 1
      if (Number.isSafeInteger(fixed) && fixed >= 0) {
        nodes += item.nodes * BigInt(fixed); decoded += item.decoded * BigInt(fixed)
      }
    } else {
      const bytes = NUMERIC.has(field.type) || field.type === 'bool'
        ? field.type === 'bool' ? 1 : Number(integerWidth(field.type)) / 8
        : typeof field.length === 'object' ? field.length.max : field.length ?? field.maxLength ?? 0
      if (Number.isSafeInteger(bytes) && bytes >= 0) decoded += BigInt(bytes) * 4n
    }
    return { nodes, decoded }
  }
  let totalNodes = 0n, totalDecoded = 0n
  for (const field of template.fields) {
    const item = budget(field, 1, field.name ?? '')
    totalNodes += item.nodes; totalDecoded += item.decoded
    if (totalNodes > 10000n) add(field.name ?? 'template', 'The expanded-result limit is 10000 nodes.')
    if (totalDecoded > 16777216n) add(field.name ?? 'template', 'The decoded-data limit is 16 MiB.')
  }
  return issues
}
