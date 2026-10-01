import { describe, expect, it } from 'vitest'
import { MinimapGlyphCache } from './minimapGlyphs'

describe('minimap pixel glyph cache', () => {
  it.each([[3, 1], [3, 2], [3, 5], [4, 6]])('retains non-space ASCII and binary punctuation in a %i×%i raster', (width, height) => {
    const cache = new MinimapGlyphCache()
    for (let byte = 0; byte < 256; byte += 1) {
      const runs = cache.get(byte, width, height)
      if (byte === 0x20) expect(runs).toHaveLength(0)
      else expect(runs.length, `byte ${byte}`).toBeGreaterThan(0)
      expect(runs.every((run) => run.x >= 0 && run.y >= 0 && run.x + run.width <= width && run.y < height)).toBe(true)
    }
  })

  it('reuses cached glyphs and maps nonprintable bytes to the same dot without a new raster', () => {
    const cache = new MinimapGlyphCache()
    expect(cache.get(0x41, 3, 5)).toBe(cache.get(0x41, 3, 5))
    expect(cache.get(0, 3, 5)).toBe(cache.get(0xff, 3, 5))
    expect(cache.get(0xff, 3, 5)).toEqual([{ x: 1, y: 4, width: 1 }])
  })

  it('evicts old raster sizes under cache pressure and releases them on disposal', () => {
    const cache = new MinimapGlyphCache()
    const first = cache.get(0x41, 3, 5)
    for (let byte = 0x20; byte <= 0x7e; byte += 1) {
      for (let width = 1; width <= 3; width += 1) {
        for (let height = 1; height <= 5; height += 1) cache.get(byte, width, height)
      }
    }
    const later = cache.get(0x41, 3, 5)
    expect(later).not.toBe(first)
    expect(later).toEqual(first)
    cache.clear()
    expect(cache.get(0x41, 3, 5)).not.toBe(later)
  })
})
