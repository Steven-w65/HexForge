import { describe, expect, it } from 'vitest'
import type { TemplateDefinition } from '../types'
import { referenceCandidates } from './references'

describe('reference suggestions', () => {
  const template: TemplateDefinition = { name: 'T', defaultEndianness: 'little', fields: [
    { name: 'count', type: 'u8' }, { name: 'title', type: 'string', encoding: 'utf8', length: 4 },
    { name: 'records', type: 'array', count: { ref: 'count' }, element: { type: 'struct', fields: [
      { name: 'size', type: 'u16' }, { name: 'data', type: 'bytes', length: { ref: 'size', max: 16 } },
    ] } }, { name: 'later', type: 'u32' },
  ] }
  it('offers earlier unsigned fields in the current and enclosing scopes, never later or unrelated repeated scopes', () => {
    expect(referenceCandidates(template, 'records[].data', 'unsigned').map(item => item.ref)).toEqual(['size', 'count'])
    expect(referenceCandidates(template, 'records', 'unsigned').map(item => item.ref)).toEqual(['count'])
    expect(referenceCandidates(template, 'later', 'unsigned').map(item => item.ref)).toEqual(['count'])
  })
  it('filters condition references by their literal type and excludes the current field', () => {
    expect(referenceCandidates(template, 'records', 'string').map(item => item.ref)).toEqual(['title'])
    expect(referenceCandidates(template, 'count', 'unsigned')).toEqual([])
  })
})
