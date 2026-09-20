import { describe, expect, it } from 'vitest'
import { contentWidth, createLayout, hitTestByte, visibleRange } from './layout'

describe('hex layout', () => {
  it('maps hex and ASCII cells to the same byte', () => {
    const layout = createLayout(900, 16)
    expect(hitTestByte(layout, layout.hexX + layout.byteStride * 3 + 2, layout.headerHeight + 4, 32n)).toBe(35n)
    expect(hitTestByte(layout, layout.asciiX + layout.charWidth * 3 + 2, layout.headerHeight + 4, 32n)).toBe(35n)
  })

  it('rejects headers, outside cells, and the grouping gap', () => {
    const layout = createLayout(900, 16)
    expect(hitTestByte(layout, layout.hexX + 2, layout.headerHeight - 1, 0n)).toBeNull()
    expect(hitTestByte(layout, layout.hexX - 1, layout.headerHeight + 4, 0n)).toBeNull()
    expect(hitTestByte(layout, layout.hexX + layout.byteStride * 7 + layout.charWidth * 2 + 1, layout.headerHeight + 4, 0n)).toBeNull()
  })

  it('keeps the grouping gap at 8px when the character width is measured', () => {
    const layout = createLayout(900, 16, 10)
    expect(layout.groupGap).toBe(8)
    expect(hitTestByte(layout, layout.hexX + layout.byteStride * 8 + 8 + 1, layout.headerHeight + 4, 0n)).toBe(8n)
    expect(hitTestByte(layout, layout.hexX + layout.byteStride * 7 + layout.byteCellWidth + 4, layout.headerHeight + 4, 0n)).toBeNull()
  })

  it('rejects bytes beyond a short final row when file size is supplied', () => {
    const layout = createLayout(900, 16)
    expect(hitTestByte(layout, layout.hexX + layout.byteStride * 3 + 2, layout.headerHeight + 4, 16n, 20n)).toBe(19n)
    expect(hitTestByte(layout, layout.hexX + layout.byteStride * 4 + 2, layout.headerHeight + 4, 16n, 20n)).toBeNull()
  })

  it('returns a prefetched, file-clamped range', () => {
    const layout = createLayout(900, 16)
    expect(visibleRange(layout, 2n, 45, 100n)).toEqual({
      firstRow: 0n,
      rowCount: 7,
      byteStart: 0n,
      byteLength: 100,
    })
  })

  it('handles empty files and caps large reads at one MiB', () => {
    const layout = createLayout(900, 32)
    expect(visibleRange(layout, 0n, 100, 0n)).toEqual({ firstRow: 0n, rowCount: 0, byteStart: 0n, byteLength: 0 })
    const result = visibleRange(layout, 0n, 1_000_000, 10_000_000n)
    expect(result.byteLength).toBe(1024 * 1024)
    expect(result.byteLength).toBeLessThanOrEqual(1024 * 1024)
  })

  it('requires bigint scroll rows and caps huge viewport spans before conversion', () => {
    const layout = createLayout(900, 32)
    expect(() => visibleRange(layout, 1 as never, 100, 10_000_000n)).toThrow()
    const scrollRow = 9_007_199_254_740_993n
    const result = visibleRange(layout, scrollRow, Number.MAX_VALUE, (scrollRow + 100_000n) * 32n)
    expect(result.firstRow).toBe(scrollRow - 2n)
    expect(result.rowCount).toBe(Math.floor((1024 * 1024) / 32))
    expect(result.byteLength).toBe(result.rowCount * layout.bytesPerRow)
  })

  it('computes hit-test offsets with bigint row arithmetic', () => {
    const layout = createLayout(900, 16)
    expect(hitTestByte(layout, layout.hexX + layout.byteStride * 3 + 2, layout.headerHeight + layout.rowHeight * 3 + 4, 9_007_199_254_740_993n)).toBe(9_007_199_254_741_044n)
    expect(hitTestByte(layout, layout.hexX + 2, layout.headerHeight + Number.MAX_SAFE_INTEGER * layout.rowHeight * 2, 0n)).toBeNull()
  })

  it('reports the full Offset/Hex/ASCII extent for horizontal reachability', () => {
    const sixteen = createLayout(500, 16); const thirtyTwo = createLayout(500, 32)
    expect(contentWidth(sixteen)).toBeGreaterThan(sixteen.asciiX + sixteen.asciiWidth)
    expect(contentWidth(thirtyTwo)).toBeGreaterThan(contentWidth(sixteen))
    expect(contentWidth(thirtyTwo)).toBeGreaterThan(500)
  })
})
