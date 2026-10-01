import { describe, expect, it } from 'vitest'
import type { TemplateDefinition } from '../types'
import { brokenReferences, renameField } from './editing'
import { fieldId, inheritFieldIdentity } from './editorState'

const definition = (fields: TemplateDefinition['fields']): TemplateDefinition => ({ name: 'Test', defaultEndianness: 'little', fields })

describe('reference-aware template editing', () => {
  it('renames only references bound to the target in their lexical scope', () => {
    const original = definition([
      { name: 'size', type: 'u8' },
      { name: 'header', type: 'struct', fields: [
        { name: 'size', type: 'u16' },
        { name: 'data', type: 'bytes', length: { ref: 'size', max: 32 }, placement: { mode: 'absolute', offset: { ref: 'size' } }, condition: { ref: 'size', op: 'gt', value: { type: 'unsigned', value: '0' } } },
      ] },
      { name: 'entries', type: 'array', count: { ref: 'header.size' }, element: { type: 'u8' } },
      { name: 'tail', type: 'bytes', length: { ref: 'size', max: 32 } },
    ])
    const updated = renameField(original, fieldId(original.fields[1]!.fields![0]!), 'length')
    expect(updated.fields[1]!.fields![1]).toMatchObject({ length: { ref: 'length' }, placement: { offset: { ref: 'length' } }, condition: { ref: 'length' } })
    expect(updated.fields[2]!.count?.ref).toBe('header.length')
    expect(updated.fields[3]!.length).toEqual({ ref: 'size', max: 32 })
    expect(original.fields[1]!.fields![0]!.name).toBe('size')
    expect(fieldId(updated.fields[1]!.fields![0]!)).toBe(fieldId(original.fields[1]!.fields![0]!))
  })

  it('renames containers and preserves concrete indices in nested-array references', () => {
    const original = definition([
      { name: 'records', type: 'array', count: { fixed: 3 }, element: { type: 'struct', fields: [
        { name: 'size', type: 'u8' }, { name: 'data', type: 'bytes', length: { ref: 'size', max: 8 } },
      ] } },
      { name: 'tail', type: 'bytes', length: { ref: 'records[2].size', max: 8 } },
    ])
    const changed = renameField(original, fieldId(original.fields[0]!.element!.fields![0]!), 'length')
    expect(changed.fields[0]!.element!.fields![1]!.length).toEqual({ ref: 'length', max: 8 })
    expect(changed.fields[1]!.length).toEqual({ ref: 'records[2].length', max: 8 })
    const renamed = renameField(changed, fieldId(changed.fields[0]!), 'items')
    expect(renamed.fields[1]!.length).toEqual({ ref: 'items[2].length', max: 8 })
    expect(brokenReferences(original, renamed)).toEqual([])
  })

  it('rejects blank, path-like, duplicate and shadowing names without touching the template', () => {
    const original = definition([{ name: 'size', type: 'u8' }, { name: 'other', type: 'u8' }])
    for (const name of ['', 'a.b', 'other']) expect(() => renameField(original, fieldId(original.fields[0]!), name)).toThrow()
    expect(original.fields.map(field => field.name)).toEqual(['size', 'other'])
  })

  it('reports dependents on deletion, reorder and lexical retargeting, not existing errors', () => {
    const original = definition([
      { name: 'size', type: 'u8' },
      { name: 'data', type: 'bytes', length: { ref: 'size', max: 8 } },
      { name: 'invalid', type: 'bytes', length: { ref: 'missing', max: 8 } },
    ])
    expect(brokenReferences(original, definition(original.fields.slice(1)))).toEqual(['data'])
    expect(brokenReferences(original, definition([original.fields[1]!, original.fields[0]!, original.fields[2]!]))).toEqual(['data'])
    expect(brokenReferences(original, original)).toEqual([])
    const renamed = inheritFieldIdentity(original.fields[0]!, { ...original.fields[0]!, name: 'length' })
    expect(brokenReferences(original, definition([renamed, ...original.fields.slice(1)]))).toEqual(['data'])
  })

  it('detects a removed concrete array element definition without expanding the array', () => {
    const original = definition([
      { name: 'items', type: 'array', count: { fixed: 10000 }, element: { type: 'struct', fields: [{ name: 'count', type: 'u8' }] } },
      { name: 'data', type: 'bytes', length: { ref: 'items[9999].count', max: 8 } },
    ])
    expect(brokenReferences(original, definition(original.fields.slice(1)))).toEqual(['data'])
  })
})
