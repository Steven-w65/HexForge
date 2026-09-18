import { describe, expect, it } from 'vitest'
import { containsOffset, normalizeSelection } from './selection'

describe('hex selection', () => {
  it('normalizes a reverse drag inclusively', () => {
    expect(normalizeSelection(12n, 8n)).toEqual({ start: 8n, end: 12n, count: 5n })
  })

  it('keeps a single byte selection exact', () => {
    expect(normalizeSelection(7n, 7n)).toEqual({ start: 7n, end: 7n, count: 1n })
  })

  it('handles large bigint selections without number conversion', () => {
    const start = 9007199254740993n
    expect(normalizeSelection(start, start + 2n)).toEqual({
      start,
      end: start + 2n,
      count: 3n,
    })
  })

  it('tests inclusive containment', () => {
    const selection = normalizeSelection(8n, 12n)
    expect(containsOffset(selection, 8n)).toBe(true)
    expect(containsOffset(selection, 12n)).toBe(true)
    expect(containsOffset(selection, 13n)).toBe(false)
  })
})
