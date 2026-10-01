import { describe, expect, it } from 'vitest'
import type { TemplateDefinition, ParsedNode } from '../types'
import { flattenResultLeaves, hasResultDiagnostics, newTemplate, validateTemplate } from './model'

describe('template helpers', () => {
  it('accepts typed checks and bounded reference lengths and offsets without changing literal forms', () => {
    const template = { ...newTemplate(), fields: [
      { name: 'offset', type: 'u64' }, { name: 'size', type: 'u16' },
      { name: 'magic', type: 'bytes', length: 2, expect: { kind: 'equals', value: { type: 'bytes', value: '4D 5A' } } },
      { name: 'text', type: 'string', encoding: 'utf8', length: { ref: 'size', max: 256 }, placement: { mode: 'absolute', offset: { ref: 'offset', add: '2' } } },
    ] } as TemplateDefinition
    expect(validateTemplate(template)).toEqual([])
  })

  it('rejects reference bounds and expected values that do not match their fields', () => {
    const template = { ...newTemplate(), fields: [
      { name: 'size', type: 'i16' },
      { name: 'payload', type: 'bytes', length: { ref: 'size', max: 0 } },
      { name: 'value', type: 'u8', expect: { kind: 'range', min: { type: 'unsigned', value: '5' }, max: { type: 'unsigned', value: '1' } } },
      { name: 'flag', type: 'bool', expect: { kind: 'equals', value: { type: 'unsigned', value: '1' } } },
    ] } as TemplateDefinition
    expect(validateTemplate(template).map(issue => issue.path)).toEqual(expect.arrayContaining(['payload', 'value', 'flag']))
  })

  it('counts containers and nested expansion against the shared result budget', () => {
    const template: TemplateDefinition = { ...newTemplate(), fields: [{ name: 'items', type: 'array', count: { fixed: 10000 }, element: { type: 'u8' } }] }
    expect(validateTemplate(template).some(issue => issue.message.includes('10000'))).toBe(true)
    template.fields = [{ name: 'items', type: 'array', count: { fixed: 100 }, element: { type: 'array', count: { fixed: 100 }, element: { type: 'u8' } } }]
    expect(validateTemplate(template).some(issue => issue.message.includes('10000'))).toBe(true)
  })

  it('rejects the total decoded budget even when each individual read is allowed', () => {
    const template: TemplateDefinition = { ...newTemplate(), fields: [{ name: 'items', type: 'array', count: { fixed: 5 }, element: { type: 'bytes', length: 1048576 } }] }
    expect(validateTemplate(template).some(issue => issue.message.includes('16 MiB'))).toBe(true)
  })

  it('includes repeated comments in the output budget instead of accepting huge result metadata', () => {
    const template: TemplateDefinition = { ...newTemplate(), fields: [{ name: 'items', type: 'array', count: { fixed: 1000 }, element: { type: 'u8', comment: 'x'.repeat(60000) } }] }
    expect(validateTemplate(template).some(issue => issue.message.includes('16 MiB'))).toBe(true)
  })

  it('creates a clean unversioned draft', () => {
    expect(newTemplate()).toEqual({ name: 'Untitled', defaultEndianness: 'little', fields: [] })
  })

  it('flattens only complete leaves for navigation and exposes nested errors', () => {
    const leaf: ParsedNode = { kind: 'leaf', name: 'id', path: 'items[2].id', type: 'u64', offset: '9007199254740993', length: '8', value: '1', endianness: 'big', comment: '', enumLabel: null, flags: [], diagnostics: [], children: [] }
    const root: ParsedNode = { kind: 'array', name: 'items', path: 'items', type: 'array', offset: '9007199254740993', length: '8', value: null, endianness: 'big', comment: '', enumLabel: null, flags: [], diagnostics: [], children: [leaf] }
    expect(flattenResultLeaves([root])).toEqual([{ name: 'id', path: 'items[2].id', offset: '9007199254740993', length: 8, type: 'u64', value: '1' }])
    expect(hasResultDiagnostics([root])).toBe(false)
    root.children.push({ ...leaf, kind: 'error', path: 'items[3].id', value: null, diagnostics: [{ code: 'template_out_of_bounds', message: 'items[3].id: beyond file' }] })
    expect(hasResultDiagnostics([root])).toBe(true)
    expect(flattenResultLeaves([root])).toHaveLength(1)
  })

  it('validates nested field shapes, references and conditions before save or apply', () => {
    const valid: TemplateDefinition = { name: 'Records', defaultEndianness: 'little', fields: [
      { name: 'count', type: 'u8' },
      { name: 'records', type: 'array', count: { ref: 'count' }, element: { type: 'struct', fields: [{ name: 'id', type: 'u16' }] } },
      { name: 'tail', type: 'u8', condition: { ref: 'count', op: 'gt', value: { type: 'unsigned', value: '0' } } },
    ] }
    expect(validateTemplate(valid)).toEqual([])
    const invalid: TemplateDefinition = { ...valid, fields: [
      { name: 'bad', type: 'array', count: { ref: 'later' }, element: { type: 'u8' } },
      { name: 'later', type: 'i8' },
    ] }
    expect(validateTemplate(invalid).map(issue => issue.path)).toContain('bad')
  })

  it('rejects invalid scalar metadata and enum keys outside the integer width in the editor', () => {
    const template: TemplateDefinition = { name: 'Invalid metadata', defaultEndianness: 'little', fields: [
      { name: 'flag', type: 'bool', endianness: 'big' },
      { name: 'text', type: 'string', encoding: 'utf8', length: 4, endianness: 'big' },
      { name: 'code', type: 'u8', enumLabels: { '256': 'too wide' } },
      { name: 'bits', type: 'u8', bitFlags: [{ bit: 0, name: 'duplicate' }, { bit: 1, name: 'duplicate' }] },
    ] }
    expect(validateTemplate(template).map(issue => issue.path)).toEqual(expect.arrayContaining(['flag', 'text', 'code', 'bits']))
  })

  it('accepts earlier same-element references in an array of structures', () => {
    const template: TemplateDefinition = { name: 'Scoped', defaultEndianness: 'little', fields: [
      { name: 'recordCount', type: 'u8' },
      { name: 'records', type: 'array', count: { ref: 'recordCount' }, element: { type: 'struct', fields: [
        { name: 'length', type: 'u8' },
        { name: 'payload', type: 'array', count: { ref: 'length' }, element: { type: 'u8' } },
      ] } },
    ] }
    expect(validateTemplate(template)).toEqual([])
  })
})
