import { describe, expect, it } from 'vitest'
import { createMinimapGeometry, type MinimapGeometry } from './minimapGeometry'
import { createMinimapInteractionController } from './minimapInteraction'

function fixture(initial: MinimapGeometry, visible = 10n, start = 0n) {
  let geometry = initial
  let mainRow = start
  let visualTop: number | null = null
  const rebuild = () => { geometry = createMinimapGeometry({
    mode: geometry.mode, totalRows: geometry.totalRows, heightPx: geometry.heightPx,
    rowPx: geometry.rowPx, topRow: mainRow, visibleMainRows: visible, visualTopPx: visualTop,
  }) }
  rebuild()
  const controller = createMinimapInteractionController({
    geometry: () => geometry,
    visibleMainRows: () => visible,
    getMainRow: () => mainRow,
    setMainRow: (row) => { mainRow = row; rebuild() },
    setVisualTop: (top) => { visualTop = top; rebuild() },
  })
  return { controller, mainRow: () => mainRow, visualTop: () => visualTop, geometry: () => geometry }
}

describe('minimap interaction controller', () => {
  it('reaches both file endpoints when clicking a compressed fit minimap', () => {
    const g = createMinimapGeometry({ mode: 'fit', totalRows: 1000n, heightPx: 100, rowPx: 2, topRow: 0n })
    const view = fixture(g, 10n)
    view.controller.click(0)
    expect(view.mainRow()).toBe(0n)
    view.controller.click(99)
    expect(view.mainRow()).toBe(990n)
  })

  it('preserves the pointer grab offset while dragging the viewport box', () => {
    const g = createMinimapGeometry({ mode: 'fit', totalRows: 100n, heightPx: 100, rowPx: 1, topRow: 0n })
    const view = fixture(g, 10n, 30n)
    view.controller.beginViewportDrag(35, 30n)
    view.controller.dragViewport(40)
    expect(view.mainRow()).toBe(35n)
    view.controller.endDrag()
    view.controller.dragViewport(80)
    expect(view.mainRow()).toBe(35n)
  })

  it('routes wheel scrolling over the minimap to the main editor', () => {
    const g = createMinimapGeometry({ mode: 'fit', totalRows: 100n, heightPx: 100, rowPx: 2, topRow: 0n })
    const view = fixture(g, 10n, 10n)
    view.controller.wheel(44)
    expect(view.mainRow()).toBe(12n)
  })

  it('maps a noncentral proportional grab through the full editor range without snapping', () => {
    const g = createMinimapGeometry({ mode: 'proportional', totalRows: 1000n, heightPx: 100, rowPx: 2, topRow: 0n })
    const view = fixture(g, 10n)
    view.controller.beginViewportDrag(7, 0n)
    view.controller.dragViewport(47)
    expect(view.visualTop()).toBe(40)
    expect(view.mainRow()).toBe(495n)
    expect(view.geometry().tileOriginSubPx).toBe(950n * 1024n)
    view.controller.dragViewport(87)
    expect(view.visualTop()).toBe(80)
    expect(view.mainRow()).toBe(990n)
    expect(view.geometry().tileOriginSubPx).toBe(1900n * 1024n)
    view.controller.dragViewport(200)
    expect(view.mainRow()).toBe(990n)
    view.controller.dragViewport(-20)
    expect(view.visualTop()).toBe(0)
    expect(view.mainRow()).toBe(0n)
    view.controller.endDrag()
    expect(view.visualTop()).toBeNull()
  })

  it('moves a proportional box upward while retaining its starting pointer offset', () => {
    const g = createMinimapGeometry({ mode: 'proportional', totalRows: 1000n, heightPx: 100, rowPx: 2, topRow: 990n, visibleMainRows: 10n })
    const view = fixture(g, 10n, 990n)
    view.controller.beginViewportDrag(85, 990n)
    view.controller.dragViewport(45)
    expect(view.visualTop()).toBe(40)
    expect(view.mainRow()).toBe(495n)
    view.controller.dragViewport(5)
    expect(view.visualTop()).toBe(0)
    expect(view.mainRow()).toBe(0n)
  })

  it('navigates to a proportional preview row without starting a box drag', () => {
    const g = createMinimapGeometry({ mode: 'proportional', totalRows: 1000n, heightPx: 100, rowPx: 2, topRow: 0n, visibleMainRows: 10n })
    const view = fixture(g, 10n)
    view.controller.click(60)
    expect(view.mainRow()).toBe(30n)
    expect(view.visualTop()).toBeNull()
    view.controller.dragViewport(80)
    expect(view.mainRow()).toBe(30n)
  })

  it('scrolls the proportional canvas upward with the editor on minimap wheel input', () => {
    const g = createMinimapGeometry({ mode: 'proportional', totalRows: 1000n, heightPx: 100, rowPx: 2, topRow: 35n, visibleMainRows: 10n })
    const view = fixture(g, 10n, 35n)
    view.controller.wheel(44)
    expect(view.mainRow()).toBe(36n)
    const moved = view.geometry().tileOriginSubPx
    expect(moved).toBeGreaterThan(g.tileOriginSubPx)
    view.controller.wheel(-44)
    expect(view.mainRow()).toBe(35n)
    expect(view.geometry().tileOriginSubPx).toBe(g.tileOriginSubPx)
  })

  it('keeps proportional drag row arithmetic exact at huge file offsets', () => {
    const top = (1n << 56n) + 30n
    const totalRows = top + 1000n
    const g = createMinimapGeometry({ mode: 'proportional', totalRows, heightPx: 100, rowPx: 2, topRow: 0n, visibleMainRows: 10n })
    const view = fixture(g, 10n)
    view.controller.beginViewportDrag(7, 0n)
    view.controller.dragViewport(87)
    expect(view.mainRow()).toBe(totalRows - 10n)
    expect(view.geometry().tileOriginSubPx).toBe((totalRows * 2n - 100n) * 1024n)
  })

  it('keeps huge row positions in bigint during navigation', () => {
    const rows = 1n << 56n
    const g = createMinimapGeometry({ mode: 'fit', totalRows: rows, heightPx: 100, rowPx: 2, topRow: 0n })
    const view = fixture(g, 10n)
    view.controller.click(99)
    expect(view.mainRow()).toBe(rows - 10n)
  })
})
