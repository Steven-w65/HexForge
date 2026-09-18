import { describe, expect, it } from 'vitest'
import { rowToThumb, thumbToRow } from './virtualScroll'

describe('64-bit virtual scrolling', () => {
  it('maps the final bigint row exactly to the final track position', () => {
    expect(rowToThumb(9_000_000_000n, 9_000_000_001n, 1000)).toBe(1000)
    expect(thumbToRow(1000, 9_000_000_001n, 1000)).toBe(9_000_000_000n)
  })

  it('clamps both mappings and handles degenerate tracks', () => {
    expect(rowToThumb(-1n, 10n, 1000)).toBe(0)
    expect(rowToThumb(99n, 10n, 1000)).toBe(1000)
    expect(thumbToRow(-5, 10n, 1000)).toBe(0n)
    expect(thumbToRow(2000, 10n, 1000)).toBe(9n)
    expect(rowToThumb(0n, 0n, 1000)).toBe(0)
    expect(rowToThumb(0n, 1n, 1000)).toBe(0)
    expect(rowToThumb(2n, 10n, 0)).toBe(0)
    expect(thumbToRow(2, 10n, 0)).toBe(0n)
  })

  it('stays monotonic at the midpoint beyond safe integer precision', () => {
    const totalRows = 18_014_398_509_481_731n
    const before = rowToThumb(9_007_199_254_740_864n, totalRows, 1000)
    const after = rowToThumb(9_007_199_254_740_865n, totalRows, 1000)
    expect(after).toBeGreaterThanOrEqual(before)
    expect(thumbToRow(500, totalRows, 1000)).toBeLessThan(totalRows)
  })
})
