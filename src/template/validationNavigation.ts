import type { TemplateField } from '../types'
import type { TemplateIssue } from './model'

export function isTemplateNameIssue(issue: TemplateIssue): boolean {
  // 'template' is also a legal binary field name, not a reserved path.
  return issue.path === 'template' && issue.message === 'Template name must not be empty.'
}

/** Editor control hints for the existing validator's diagnostics. Keep these
 * UI concerns outside the JSON model and authoritative backend validation. */
export function validationControl(issue: TemplateIssue, field?: TemplateField): string {
  const text = issue.message
  if (isTemplateNameIssue(issue)) return 'template-name'
  if (/Field name/i.test(text)) return 'name'
  if (/Alignment/.test(text)) return 'align'
  if (/Offset addition/.test(text)) return 'offset-add'
  if (/^Offset/.test(text)) return 'offset-ref'
  if (/Placement offset/.test(text)) return 'placement-offset'
  if (/Sequential placement/.test(text)) return 'placement-mode'
  if (/^Condition integer/.test(text)) return 'condition-value'
  if (/Boolean\/text conditions/.test(text)) return 'condition-op'
  if (/^Condition/.test(text)) return 'condition-ref'
  if (/^Length/.test(text)) return 'length-ref'
  if (/^Array count/.test(text)) return 'count-ref'
  if (/Fixed array count|expanded-result limit/.test(text) && field?.type === 'array') return 'count-fixed'
  if (/Bit flags/.test(text)) return 'flag-bit'
  if (/Enum/.test(text)) return 'enum-key'
  if (/Expected range minimum/.test(text)) return 'expect-min'
  if (/Expected/.test(text)) return field?.expect?.kind === 'oneOf' ? 'expect-allowed' : field?.expect?.kind === 'range' ? 'expect-min' : 'expect-value'
  if (/UTF-16|Text needs|Bytes require/.test(text)) return typeof field?.length === 'object' ? 'length-max' : field?.type === 'string' ? 'text-length' : 'length'
  return 'type'
}

export interface ValidationTarget { control: string; index?: number }
export function validationTarget(issue: TemplateIssue, field?: TemplateField): ValidationTarget {
  const width = Number(field?.type.match(/^[ui](8|16|32|64)$/)?.[1] ?? 0)
  if (/Enum/.test(issue.message) && field?.enumLabels) {
    if (!width) return { control: 'type' }
    for (const [index, [key, label]] of Object.entries(field.enumLabels).entries()) {
      let fits = false
      if (/^-?\d+$/.test(key) && !(field.type.startsWith('u') && key.startsWith('-'))) {
        const value = BigInt(key), bits = BigInt(width)
        fits = field.type.startsWith('u') ? value >= 0n && value < (1n << bits) : value >= -(1n << (bits - 1n)) && value < (1n << (bits - 1n))
      }
      if (!fits) return { control: 'enum-key', index }
      if (!label.trim()) return { control: 'enum-label', index }
    }
  }
  if (/Bit flags/.test(issue.message) && field?.bitFlags) {
    if (!width || !field.type.startsWith('u')) return { control: 'type' }
    const bits = new Set<number>(), names = new Set<string>()
    for (const [index, flag] of field.bitFlags.entries()) {
      if (!Number.isSafeInteger(flag.bit) || flag.bit < 0 || flag.bit >= width || bits.has(flag.bit)) return { control: 'flag-bit', index }
      if (!flag.name.trim() || names.has(flag.name)) return { control: 'flag-name', index }
      bits.add(flag.bit); names.add(flag.name)
    }
  }
  return { control: validationControl(issue, field) }
}

export interface FieldReveal extends ValidationTarget { id: string; sequence: number }
