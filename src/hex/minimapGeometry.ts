export type MinimapMode = 'fit' | 'proportional'

export interface MinimapSettings {
  enabled: boolean
  mode: MinimapMode
  renderCharacters: boolean
  scale: 1 | 2 | 3
}

export const DEFAULT_MINIMAP_SETTINGS: MinimapSettings = {
  enabled: true, mode: 'fit', renderCharacters: true, scale: 1,
}

export interface MinimapGeometry {
  mode: MinimapMode
  totalRows: bigint
  heightPx: number
  rowPx: number
  topRow: bigint
  visibleMiniRows: bigint
  logicalHeightPx: bigint
  canvasHeightPx: number
  fitsNaturally: boolean
  /** Main editor scroll range and proportional slider state (fixed-point CSS pixels). */
  mainTopRow: bigint
  visibleMainRows: bigint
  maxMainRow: bigint
  viewportHeightPx: number
  viewportTravelPx: number
  viewportTopSubPx: bigint
  /** Logical canvas origin; the backing bitmap is still only canvasHeightPx high. */
  tileOriginSubPx: bigint
}

export interface MinimapGeometryInput {
  mode: MinimapMode
  totalRows: bigint
  heightPx: number
  rowPx: number
  topRow: bigint
  visibleMainRows?: bigint
  /** Active-drag box top in container pixels. Ignored in fit mode. */
  visualTopPx?: number | null
}

export interface MinimapSamplePosition {
  y: number
  row: bigint
  height: number
}

export interface MinimapViewportBox {
  top: number
  height: number
  // Exact canvas-local pixel position for very large proportional files.
  topPx?: bigint
  topSubPx?: bigint
}

export const MINIMAP_SUBPIXELS = 1024n

function clampRow(row: bigint, maximum: bigint): bigint {
  return row < 0n ? 0n : row > maximum ? maximum : row
}

export function createMinimapGeometry(input: MinimapGeometryInput): MinimapGeometry {
  const { mode, totalRows, heightPx, rowPx } = input
  if (mode !== 'fit' && mode !== 'proportional') throw new RangeError('Invalid minimap mode')
  if (!Number.isSafeInteger(heightPx) || heightPx < 1) throw new RangeError('Invalid minimap height')
  if (!Number.isSafeInteger(rowPx) || rowPx < 1) throw new RangeError('Minimap row height must be positive')
  if (totalRows < 0n || input.topRow < 0n) throw new RangeError('Invalid minimap row range')

  // Integer ceiling avoids floating-point drift at small row heights.
  const visibleMiniRows = (BigInt(heightPx) + BigInt(rowPx) - 1n) / BigInt(rowPx)
  const naturalPx = totalRows * BigInt(rowPx)
  const fitsNaturally = naturalPx <= BigInt(heightPx)
  // Both modes keep short files at native row density. Long proportional
  // documents use a viewport-sized backing tile, not a giant bitmap.
  const canvasHeightPx = fitsNaturally ? Number(naturalPx) : heightPx
  const lastTop = totalRows > visibleMiniRows ? totalRows - visibleMiniRows : 0n
  const visibleMainRows = input.visibleMainRows ?? visibleMiniRows
  if (visibleMainRows < 1n) throw new RangeError('Visible editor rows must be positive')
  const maxMainRow = totalRows > visibleMainRows ? totalRows - visibleMainRows : 0n
  const mainTopRow = clampRow(input.topRow, maxMainRow)
  const viewportHeightPx = mode === 'proportional'
    ? Number(naturalPx < visibleMainRows * BigInt(rowPx) ? naturalPx :
      BigInt(canvasHeightPx) < visibleMainRows * BigInt(rowPx) ? BigInt(canvasHeightPx) : visibleMainRows * BigInt(rowPx))
    : 0
  const viewportTravelPx = mode === 'proportional' ? Math.max(0, canvasHeightPx - viewportHeightPx) : 0
  let viewportTopSubPx = 0n
  if (mode === 'proportional' && viewportTravelPx > 0 && maxMainRow > 0n) {
    if (input.visualTopPx !== null && input.visualTopPx !== undefined && Number.isFinite(input.visualTopPx)) {
      const bounded = Math.max(0, Math.min(viewportTravelPx, input.visualTopPx))
      viewportTopSubPx = BigInt(Math.round(bounded * Number(MINIMAP_SUBPIXELS)))
    } else {
      viewportTopSubPx = mainTopRow * BigInt(viewportTravelPx) * MINIMAP_SUBPIXELS / maxMainRow
    }
  }
  // The tile and box are projections of one position. In zero-travel cases
  // the box fills the column, but the editor can still scroll the tile.
  const scrollableLogicalPx = naturalPx - BigInt(canvasHeightPx)
  const tileOriginSubPx = mode !== 'proportional' || maxMainRow === 0n ? 0n : viewportTravelPx > 0
    ? scrollableLogicalPx * viewportTopSubPx / BigInt(viewportTravelPx)
    : scrollableLogicalPx * mainTopRow * MINIMAP_SUBPIXELS / maxMainRow
  const sampleTopRow = mode === 'proportional' ? tileOriginSubPx / (BigInt(rowPx) * MINIMAP_SUBPIXELS) : 0n
  return {
    mode,
    totalRows,
    heightPx,
    rowPx,
    topRow: mode === 'proportional' ? clampRow(sampleTopRow, lastTop) : 0n,
    visibleMiniRows,
    logicalHeightPx: naturalPx,
    canvasHeightPx,
    fitsNaturally,
    mainTopRow,
    visibleMainRows,
    maxMainRow,
    viewportHeightPx,
    viewportTravelPx,
    viewportTopSubPx,
    tileOriginSubPx,
  }
}

/** Returns the logical top edge: fit uses CSS pixels; proportional uses exact 1/1024px fixed point. */
export function rowEdgeY(g: MinimapGeometry, row: bigint): number | bigint {
  if (g.totalRows === 0n) return g.mode === 'proportional' ? 0n : 0
  const bounded = clampRow(row, g.totalRows)
  if (g.mode === 'fit') {
    if (g.fitsNaturally) return Number(bounded * BigInt(g.rowPx))
    // Bigint floor division is essential when the source row exceeds 2^53.
    return Number(bounded * BigInt(g.heightPx) / g.totalRows)
  }
  // Never coerce a huge logical canvas coordinate to Number. Its consumer
  // subtracts tileOriginSubPx before converting a viewport-bounded delta.
  return bounded * BigInt(g.rowPx) * MINIMAP_SUBPIXELS
}

/** Fixed-point canvas-local → container offset; exact even beyond 2^53 rows. */
export function canvasOffsetY(g: MinimapGeometry): bigint {
  return g.mode === 'proportional' ? -g.tileOriginSubPx : 0n
}

/** Inverse Y mapping. Compressed bins choose a representative source row. */
export function rowAtY(g: MinimapGeometry, y: number | bigint): bigint {
  if (g.totalRows === 0n || g.canvasHeightPx === 0) return 0n
  if (g.mode === 'proportional') {
    // Bigint input is the exact fixed-point value returned by rowEdgeY.
    const subPx = typeof y === 'bigint' ? y : BigInt(Math.floor((Number.isFinite(y) ? y : 0) * Number(MINIMAP_SUBPIXELS)))
    return clampRow(subPx / (BigInt(g.rowPx) * MINIMAP_SUBPIXELS), g.totalRows - 1n)
  }
  const finiteY = typeof y === 'bigint' ? Number(y) : Number.isFinite(y) ? y : 0
  const py = BigInt(Math.max(0, Math.min(g.canvasHeightPx - 1, Math.floor(finiteY))))
  if (g.fitsNaturally) return clampRow(py / BigInt(g.rowPx), g.totalRows - 1n)
  if (py === 0n) return 0n
  if (py === BigInt(g.canvasHeightPx - 1)) return g.totalRows - 1n
  const candidate = ((2n * py + 1n) * g.totalRows) / (2n * BigInt(g.canvasHeightPx))
  return clampRow(candidate, g.totalRows - 1n)
}

export function viewportBox(
  g: MinimapGeometry,
  visibleStart: bigint,
  visibleEndExclusive: bigint,
): MinimapViewportBox | null {
  if (g.totalRows === 0n || g.canvasHeightPx === 0) return null
  if (g.mode === 'proportional') {
    // During a drag, the main editor is quantized to rows while the box
    // remains at the continuous pointer position.
    const topSubPx = g.tileOriginSubPx + g.viewportTopSubPx
    return { top: Number(g.viewportTopSubPx) / Number(MINIMAP_SUBPIXELS), height: g.viewportHeightPx,
      topPx: topSubPx / MINIMAP_SUBPIXELS, topSubPx }
  }
  const top = Math.max(0, Math.min(g.canvasHeightPx, Number(rowEdgeY(g, visibleStart))))
  // rowEdgeY returns the *top* of a row interval, so the end-exclusive
  // row's top is exactly the viewport's bottom boundary.
  const bottom = Math.max(top, Math.min(g.canvasHeightPx, Number(rowEdgeY(g, visibleEndExclusive))))
  if (g.mode !== 'fit' || g.fitsNaturally) return { top, height: bottom - top }
  const minHeight = Math.min(2, g.canvasHeightPx)
  const shownTop = Math.min(top, g.canvasHeightPx - minHeight)
  return { top: shownTop, height: Math.max(minHeight, bottom - shownTop) }
}

/** A bounded rendering plan: source rows never determine Canvas allocation. */
export function samplePlan(g: MinimapGeometry): MinimapSamplePosition[] {
  if (g.totalRows === 0n || g.canvasHeightPx === 0) return []
  const plan: MinimapSamplePosition[] = []
  if (g.mode === 'fit' && !g.fitsNaturally) {
    for (let y = 0; y < g.canvasHeightPx; y += 1) plan.push({ y, row: rowAtY(g, y), height: 1 })
    return plan
  }

  if (g.mode === 'proportional') {
    const stride = BigInt(g.rowPx) * MINIMAP_SUBPIXELS
    const endSubPx = g.tileOriginSubPx + BigInt(g.canvasHeightPx) * MINIMAP_SUBPIXELS
    const endRow = (endSubPx + stride - 1n) / stride
    const max = endRow < g.totalRows ? endRow : g.totalRows
    for (let row = g.topRow; row < max; row += 1n) {
      const top = row * stride - g.tileOriginSubPx
      const bottom = top + stride
      const clippedTop = top > 0n ? top : 0n
      const canvasBottom = BigInt(g.canvasHeightPx) * MINIMAP_SUBPIXELS
      const clippedBottom = bottom < canvasBottom ? bottom : canvasBottom
      if (clippedBottom > clippedTop) plan.push({
        y: Number(clippedTop) / Number(MINIMAP_SUBPIXELS), row,
        height: Number(clippedBottom - clippedTop) / Number(MINIMAP_SUBPIXELS),
      })
    }
    return plan
  }
  const first = 0n
  const max = g.totalRows
  for (let row = first; row < max; row += 1n) {
    // The backing bitmap is a bounded tile; convert logical canvas Y only
    // after bigint subtraction to avoid precision loss at large offsets.
    const y = Number(rowEdgeY(g, row))
    if (y >= g.canvasHeightPx) break
    plan.push({ y, row, height: Math.min(g.rowPx, g.canvasHeightPx - y) })
  }
  return plan
}
