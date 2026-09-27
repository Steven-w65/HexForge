import { describe, expect, it } from 'vitest'
import { canvasOffsetY, createMinimapGeometry, rowAtY, rowEdgeY, samplePlan, viewportBox } from './minimapGeometry'

describe('minimap row geometry', () => {
  it('rejects a zero or non-integer row height before division', () => {
    expect(() => createMinimapGeometry({ mode: 'fit', totalRows: 2n, heightPx: 100, rowPx: 0, topRow: 0n })).toThrow(RangeError)
    expect(() => createMinimapGeometry({ mode: 'fit', totalRows: 2n, heightPx: 100, rowPx: 0.5, topRow: 0n })).toThrow(RangeError)
  })

  it('keeps short fit content at its natural height without stretching', () => {
    const g = createMinimapGeometry({ mode: 'fit', totalRows: 2n, heightPx: 100, rowPx: 4, topRow: 0n })
    expect(g.canvasHeightPx).toBe(8)
    expect(rowEdgeY(g, 0n)).toBe(0)
    expect(rowEdgeY(g, 2n)).toBe(8)
    expect(rowAtY(g, 7)).toBe(1n)
    expect(rowAtY(g, 99)).toBe(1n)
    expect(samplePlan(g)).toEqual([{ y: 0, row: 0n, height: 4 }, { y: 4, row: 1n, height: 4 }])
  })

  it('compresses huge row counts without losing the last row to floating point rounding', () => {
    const totalRows = 1n << 56n
    const g = createMinimapGeometry({ mode: 'fit', totalRows, heightPx: 500, rowPx: 2, topRow: 0n })
    expect(g.canvasHeightPx).toBe(500)
    expect(rowEdgeY(g, totalRows)).toBe(500)
    expect(rowAtY(g, 0)).toBe(0n)
    expect(rowAtY(g, 499)).toBe(totalRows - 1n)
    expect(samplePlan(g)).toHaveLength(500)
  })

  it('uses raw canvas-local coordinates and exact integer ceiling in proportional mode', () => {
    const g = createMinimapGeometry({ mode: 'proportional', totalRows: 100n, heightPx: 101, rowPx: 3, topRow: 10n })
    expect(g.visibleMiniRows).toBe(34n)
    expect(g.canvasHeightPx).toBe(101)
    expect(g.logicalHeightPx).toBe(300n)
    expect(rowEdgeY(g, 10n)).toBe(30n * 1024n)
    expect(rowAtY(g, 35)).toBe(11n)
    expect(rowEdgeY(g, 9n)).toBe(27n * 1024n)
    expect(rowEdgeY(g, 45n)).toBe(135n * 1024n)
    expect(canvasOffsetY(g)).toBe(-g.tileOriginSubPx)
    expect(viewportBox(g, 12n, 20n)?.height).toBe(101)
    expect(samplePlan(g)[0]?.row).toBe(g.topRow)
  })

  it('keeps canvas-local inverse mapping exact above Number.MAX_SAFE_INTEGER', () => {
    const topRow = (1n << 56n) + 17n
    const g = createMinimapGeometry({ mode: 'proportional', totalRows: topRow + 100n, heightPx: 100, rowPx: 2, topRow })
    expect(g.logicalHeightPx).toBe((topRow + 100n) * 2n)
    expect(rowAtY(g, (topRow + 3n) * 2n * 1024n)).toBe(topRow + 3n)
    expect(viewportBox(g, topRow + 3n, topRow + 13n)?.height).toBe(100)
  })

  it('uses the natural logical canvas height for a short proportional file', () => {
    const g = createMinimapGeometry({ mode: 'proportional', totalRows: 2n, heightPx: 100, rowPx: 4, topRow: 0n })
    expect(g.logicalHeightPx).toBe(8n)
    expect(g.canvasHeightPx).toBe(8)
    expect(samplePlan(g)).toEqual([{ y: 0, row: 0n, height: 4 }, { y: 4, row: 1n, height: 4 }])
  })

  it('maps the editor range to a bounded proportional box and tile without losing EOF', () => {
    const at = (topRow: bigint) => createMinimapGeometry({
      mode: 'proportional', totalRows: 1000n, heightPx: 100, rowPx: 2,
      topRow, visibleMainRows: 10n,
    })
    const top = at(0n)
    const middle = at(495n)
    const bottom = at(990n)
    expect(top.canvasHeightPx).toBe(100)
    expect(top.viewportHeightPx).toBe(20)
    expect(top.viewportTravelPx).toBe(80)
    expect(top.viewportTopSubPx).toBe(0n)
    expect(top.tileOriginSubPx).toBe(0n)
    expect(middle.viewportTopSubPx).toBe(40n * 1024n)
    expect(middle.tileOriginSubPx).toBe(950n * 1024n)
    expect(bottom.viewportTopSubPx).toBe(80n * 1024n)
    expect(bottom.tileOriginSubPx).toBe(1900n * 1024n)
    expect(samplePlan(bottom).at(-1)?.row).toBe(999n)
    expect(samplePlan(bottom).at(-1)?.y).toBe(98)
  })

  it('renders partial edge rows within a fractional proportional tile', () => {
    const g = createMinimapGeometry({
      mode: 'proportional', totalRows: 1000n, heightPx: 101, rowPx: 2,
      topRow: 495n, visibleMainRows: 10n, visualTopPx: 40.25,
    })
    const plan = samplePlan(g)
    expect(plan[0]!.y).toBe(0)
    expect(plan[0]!.height).toBeGreaterThan(0)
    expect(plan[0]!.height).toBeLessThan(2)
    expect(plan.at(-1)!.y + plan.at(-1)!.height).toBe(101)
    expect(plan.length).toBe(52)
  })

  it('keeps a short proportional file natural and handles zero travel and empty files', () => {
    const short = createMinimapGeometry({ mode: 'proportional', totalRows: 20n, heightPx: 100, rowPx: 2, topRow: 10n, visibleMainRows: 10n })
    expect(short.canvasHeightPx).toBe(40)
    expect(short.viewportTopSubPx).toBe(20n * 1024n)
    expect(short.tileOriginSubPx).toBe(0n)
    const noTravel = createMinimapGeometry({ mode: 'proportional', totalRows: 100n, heightPx: 20, rowPx: 2, topRow: 90n, visibleMainRows: 10n })
    expect(noTravel.viewportTravelPx).toBe(0)
    expect(noTravel.tileOriginSubPx).toBe(180n * 1024n)
    const empty = createMinimapGeometry({ mode: 'proportional', totalRows: 0n, heightPx: 100, rowPx: 2, topRow: 0n, visibleMainRows: 10n })
    expect(empty.canvasHeightPx).toBe(0)
    expect(viewportBox(empty, 0n, 0n)).toBeNull()
  })

  it('keeps very large proportional tile origins exact', () => {
    const totalRows = (1n << 56n) + 1000n
    const g = createMinimapGeometry({ mode: 'proportional', totalRows, heightPx: 100, rowPx: 2,
      topRow: totalRows - 10n, visibleMainRows: 10n })
    expect(g.tileOriginSubPx).toBe((totalRows * 2n - 100n) * 1024n)
    expect(samplePlan(g).at(-1)?.row).toBe(totalRows - 1n)
    expect(rowAtY(g, (totalRows - 1n) * 2n * 1024n)).toBe(totalRows - 1n)
  })

  it('round-trips a very large canvas-local row edge without Number precision loss', () => {
    const row = (1n << 56n) + 17n
    const g = createMinimapGeometry({ mode: 'proportional', totalRows: row + 100n,
      heightPx: 100, rowPx: 2, topRow: row, visibleMainRows: 10n })
    const edge = rowEdgeY(g, row)
    expect(edge).toBe(row * 2n * 1024n)
    expect(rowAtY(g, edge)).toBe(row)
  })

  it('uses the top edge of the end-exclusive row for the viewport bottom', () => {
    const g = createMinimapGeometry({ mode: 'fit', totalRows: 10n, heightPx: 100, rowPx: 4, topRow: 0n })
    expect(viewportBox(g, 2n, 5n)).toEqual({ top: 8, height: 12 })
    expect(rowEdgeY(g, 5n)).toBe(20)
  })

  it('gives the first endpoint precedence for a one-pixel compressed canvas', () => {
    const g = createMinimapGeometry({ mode: 'fit', totalRows: 10n, heightPx: 1, rowPx: 2, topRow: 0n })
    expect(rowAtY(g, 0)).toBe(0n)
    expect(samplePlan(g)).toEqual([{ y: 0, row: 0n, height: 1 }])
  })

  it('keeps an empty file empty', () => {
    const g = createMinimapGeometry({ mode: 'fit', totalRows: 0n, heightPx: 100, rowPx: 2, topRow: 0n })
    expect(g.canvasHeightPx).toBe(0)
    expect(rowAtY(g, 50)).toBe(0n)
    expect(samplePlan(g)).toEqual([])
    expect(viewportBox(g, 0n, 0n)).toBeNull()
  })
})
