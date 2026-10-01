import { describe, expect, it } from 'vitest'
import { duplicateTemplateField } from './model'

describe('template field duplication', () => {
  it('copies a nested field after its source with an independent tree and unique sibling name', () => {
    const fields = [
      { name: 'record', type: 'struct' as const, fields: [{ name: 'id', type: 'u64' as const }] },
      { name: 'recordCopy', type: 'u8' as const },
    ]
    const duplicated = duplicateTemplateField(fields, 0)
    expect(duplicated.map(field => field.name)).toEqual(['record', 'recordCopy2', 'recordCopy'])
    expect(duplicated[1]).toEqual({ name: 'recordCopy2', type: 'struct', fields: [{ name: 'id', type: 'u64' }] })
    duplicated[1]!.fields![0]!.name = 'changed'
    expect(fields[0]!.fields?.[0]!.name).toBe('id')
  })
})
