import type { ModifiedOverview } from '../types'
import type { BytesPerRow } from './layout'
import { MINIMAP_SUBPIXELS, rowEdgeY, type MinimapGeometry } from './minimapGeometry'

export type MinimapMarkerKind = 'search' | 'template' | 'modified'

export interface MinimapMarker {
  kind: MinimapMarkerKind
  row: bigint
  endRowExclusive: bigint
}

export interface MinimapPixelRun {
  kind: MinimapMarkerKind
  top: number
  height: number
}

const PRIORITY: MinimapMarkerKind[] = ['search', 'template', 'modified']

function clampOffset(offset: bigint, fileSize: bigint): bigint {
  return offset < 0n ? 0n : offset > fileSize ? fileSize : offset
}

/** Normalize existing binary markers once; both visual surfaces consume this value. */
export function collectMinimapMarkers(
  matches: bigint[],
  matchLength: number,
  templateFields: Array<{ offset: string; length: number }>,
  modifiedOverview: ModifiedOverview,
  fileSize: bigint,
  bytesPerRow: BytesPerRow,
): MinimapMarker[] {
  if (fileSize <= 0n) return []
  const width = BigInt(bytesPerRow)
  const totalRows = (fileSize + width - 1n) / width
  const markers: MinimapMarker[] = []
  const append = (kind: MinimapMarkerKind, rawStart: bigint, rawEnd: bigint): void => {
    const start = clampOffset(rawStart, fileSize)
    const end = clampOffset(rawEnd, fileSize)
    if (start >= end || start >= fileSize) return
    const row = start / width
    const endRowExclusive = (end + width - 1n) / width
    markers.push({ kind, row, endRowExclusive: endRowExclusive > totalRows ? totalRows : endRowExclusive })
  }

  if (Number.isSafeInteger(matchLength) && matchLength > 0) {
    for (const match of matches) append('search', match, match + BigInt(matchLength))
  }
  for (const field of templateFields) {
    if (!Number.isSafeInteger(field.length) || field.length <= 0) continue
    try {
      const start = BigInt(field.offset)
      append('template', start, start + BigInt(field.length))
    } catch { /* Incomplete template drafts do not affect the minimap. */ }
  }
  if (Number.isInteger(modifiedOverview.binCount) && modifiedOverview.binCount > 0) {
    const lastBin = BigInt(modifiedOverview.binCount - 1)
    for (const bin of modifiedOverview.bins) {
      if (!Number.isInteger(bin) || bin < 0 || bin >= modifiedOverview.binCount) continue
      const approximate = lastBin === 0n ? 0n : BigInt(bin) * (fileSize - 1n) / lastBin
      append('modified', approximate, approximate + 1n)
    }
  }

  // Merge intervals of one kind before rasterizing dense search results.
  const merged: MinimapMarker[] = []
  for (const kind of PRIORITY) {
    const sameKind = markers.filter((marker) => marker.kind === kind).sort((a, b) => a.row < b.row ? -1 : a.row > b.row ? 1 : 0)
    for (const marker of sameKind) {
      const prior = merged[merged.length - 1]
      if (prior?.kind === kind && marker.row <= prior.endRowExclusive) {
        if (marker.endRowExclusive > prior.endRowExclusive) prior.endRowExclusive = marker.endRowExclusive
      } else merged.push({ ...marker })
    }
  }
  return merged.sort((a, b) => a.row < b.row ? -1 : a.row > b.row ? 1 : PRIORITY.indexOf(a.kind) - PRIORITY.indexOf(b.kind))
}

function projectRuns(
  markers: MinimapMarker[],
  heightPx: number,
  project: (marker: MinimapMarker) => { top: number; bottom: number } | null,
): MinimapPixelRun[] {
  const height = Number.isSafeInteger(heightPx) && heightPx > 0 ? heightPx : 0
  if (height === 0) return []
  const changes = PRIORITY.map(() => new Int32Array(height + 1))
  for (const marker of markers) {
    const interval = project(marker)
    if (!interval) continue
    // Typed-array difference indices must be whole pixels. Fractional
    // proportional tile origins cover every pixel touched by a marker.
    const top = Math.max(0, Math.min(height - 1, Math.floor(interval.top)))
    const bottom = Math.max(top + 1, Math.min(height, Math.ceil(interval.bottom)))
    const diff = changes[PRIORITY.indexOf(marker.kind)]!
    diff[top]! += 1
    diff[bottom]! -= 1
  }
  const active = [0, 0, 0]
  const runs: MinimapPixelRun[] = []
  for (let y = 0; y < height; y += 1) {
    for (let index = 0; index < active.length; index += 1) active[index]! += changes[index]![y]!
    const kind = active[2]! > 0 ? 'modified' : active[1]! > 0 ? 'template' : active[0]! > 0 ? 'search' : null
    if (!kind) continue
    const last = runs[runs.length - 1]
    if (last?.kind === kind && last.top + last.height === y) last.height += 1
    else runs.push({ kind, top: y, height: 1 })
  }
  return runs
}

export function projectMinimapMarkers(markers: MinimapMarker[], geometry: MinimapGeometry): MinimapPixelRun[] {
  const g = geometry
  return projectRuns(markers, g.canvasHeightPx, (marker) => {
    if (g.mode === 'proportional') {
      const stride = BigInt(g.rowPx) * MINIMAP_SUBPIXELS
      const tileEnd = g.tileOriginSubPx + BigInt(g.canvasHeightPx) * MINIMAP_SUBPIXELS
      const markerStart = marker.row * stride
      const markerEnd = marker.endRowExclusive * stride
      if (markerEnd <= g.tileOriginSubPx || markerStart >= tileEnd) return null
      const start = markerStart > g.tileOriginSubPx ? markerStart : g.tileOriginSubPx
      const end = markerEnd < tileEnd ? markerEnd : tileEnd
      // Subtract the huge logical origin before Number conversion.
      return {
        top: Number(start - g.tileOriginSubPx) / Number(MINIMAP_SUBPIXELS),
        bottom: Number(end - g.tileOriginSubPx) / Number(MINIMAP_SUBPIXELS),
      }
    }
    return { top: Number(rowEdgeY(g, marker.row)), bottom: Number(rowEdgeY(g, marker.endRowExclusive)) }
  })
}

export function projectOverviewMarkers(markers: MinimapMarker[], totalRows: bigint, heightPx: number): MinimapPixelRun[] {
  return projectRuns(markers, heightPx, (marker) => {
    if (totalRows <= 0n) return null
    // Unlike minimap row intervals, the overview ruler pins the last row to
    // the last pixel so EOF markers remain visible at the bottom edge.
    const lastPixel = BigInt(heightPx - 1)
    const lastRow = totalRows - 1n
    const top = lastRow === 0n ? 0 : Number(marker.row * lastPixel / lastRow)
    const lastCovered = marker.endRowExclusive > 0n ? marker.endRowExclusive - 1n : 0n
    const bottom = lastRow === 0n ? 1 : Number(lastCovered * lastPixel / lastRow) + 1
    return { top, bottom }
  })
}
