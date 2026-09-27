import type { ColorTheme } from '../types'
import type { MinimapRow } from './minimapData'
import type { BytesPerRow } from './layout'
import { MINIMAP_SUBPIXELS, canvasOffsetY, samplePlan, type MinimapGeometry, type MinimapSamplePosition, type MinimapViewportBox } from './minimapGeometry'
import type { MinimapPixelRun } from './minimapMarkers'

export interface MinimapCanvasLayers {
  visible: CanvasRenderingContext2D
  base: CanvasRenderingContext2D
  markers: CanvasRenderingContext2D
}

export interface MinimapRenderOptions {
  bytesPerRow: BytesPerRow
  renderCharacters: boolean
  scale: 1 | 2 | 3
  theme: ColorTheme
}

function offscreenContext(): CanvasRenderingContext2D {
  const canvas = document.createElement('canvas')
  const context = canvas.getContext('2d')
  if (!context) throw new Error('Canvas 2D rendering is unavailable.')
  return context
}

export function createMinimapCanvasLayers(canvas: HTMLCanvasElement): MinimapCanvasLayers {
  const visible = canvas.getContext('2d')
  if (!visible) throw new Error('Canvas 2D rendering is unavailable.')
  return { visible, base: offscreenContext(), markers: offscreenContext() }
}

const COLORS = {
  dark: { background: '#161b20', zero: '#39444c', printable: '#899cac', other: '#607584' },
  light: { background: '#f3f5f7', zero: '#d8e0e6', printable: '#647b8e', other: '#9cadb9' },
} as const
const MARKER_COLORS = { search: '#bda64a', template: '#39c5cf', modified: '#f0883e' } as const

function printable(byte: number): string {
  return byte >= 0x20 && byte <= 0x7e ? String.fromCharCode(byte) : '.'
}

function sameRasterGeometry(a: MinimapGeometry, b: MinimapGeometry): boolean {
  return a.mode === b.mode && a.totalRows === b.totalRows && a.heightPx === b.heightPx &&
    a.rowPx === b.rowPx && a.canvasHeightPx === b.canvasHeightPx && a.fitsNaturally === b.fitsNaturally
}

function sameOptions(a: MinimapRenderOptions, b: MinimapRenderOptions): boolean {
  return a.bytesPerRow === b.bytesPerRow && a.renderCharacters === b.renderCharacters && a.scale === b.scale && a.theme === b.theme
}

export class MinimapRenderer {
  private width = 0
  private height = 0
  private dpr = 1
  private geometry: MinimapGeometry | null = null
  private rows: ReadonlyMap<bigint, MinimapRow> = new Map()
  private options: MinimapRenderOptions = { bytesPerRow: 16, renderCharacters: true, scale: 1, theme: 'dark' }
  private markers: MinimapPixelRun[] = []
  private viewport: MinimapViewportBox | null = null
  private viewportState: 'normal' | 'hover' | 'active' = 'normal'
  private fullBaseDirty = true
  private fullMarkersDirty = true
  private baseDirtyRows = new Set<bigint>()
  private markerDirtyRects: Array<{ top: number; bottom: number }> = []
  private scrollShiftPx = 0
  private visibleDirty = true

  constructor(private readonly layers: MinimapCanvasLayers) {}

  resize(widthPx: number, heightPx: number, dpr = globalThis.devicePixelRatio || 1): void {
    const width = Math.max(1, Math.floor(widthPx))
    const height = Math.max(1, Math.floor(heightPx))
    const pixelRatio = Number.isFinite(dpr) && dpr > 0 ? dpr : 1
    if (width === this.width && height === this.height && pixelRatio === this.dpr) return
    this.width = width
    this.height = height
    this.dpr = pixelRatio
    for (const context of Object.values(this.layers)) {
      context.canvas.width = Math.max(1, Math.round(width * pixelRatio))
      context.canvas.height = Math.max(1, Math.round(height * pixelRatio))
      context.canvas.style.width = `${width}px`
      context.canvas.style.height = `${height}px`
      context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0)
    }
    this.fullBaseDirty = true
    this.fullMarkersDirty = true
    this.visibleDirty = true
  }

  setContent(geometry: MinimapGeometry, rows: ReadonlyMap<bigint, MinimapRow>, options: MinimapRenderOptions): void {
    const oldGeometry = this.geometry
    const optionsChanged = !sameOptions(this.options, options)
    if (!oldGeometry || optionsChanged || !sameRasterGeometry(oldGeometry, geometry)) {
      // Fit geometry and theme changes repaint only this bounded visible tile.
      this.fullBaseDirty = true
      this.scrollShiftPx = 0
    } else if (oldGeometry.tileOriginSubPx !== geometry.tileOriginSubPx && !this.fullBaseDirty) {
      const delta = geometry.tileOriginSubPx - oldGeometry.tileOriginSubPx
      // A fractional shift changes every raster row's coverage. Integer
      // shifts can reuse the overlap in the viewport-sized backing canvas.
      if (delta % MINIMAP_SUBPIXELS !== 0n || delta <= -BigInt(this.height) * MINIMAP_SUBPIXELS ||
          delta >= BigInt(this.height) * MINIMAP_SUBPIXELS) this.fullBaseDirty = true
      else this.scrollShiftPx = Number(delta / MINIMAP_SUBPIXELS)
    }
    if (oldGeometry && !sameRasterGeometry(oldGeometry, geometry)) this.fullMarkersDirty = true
    // A file switch can keep identical geometry while dropping all cached
    // samples. Repaint those rows to background before new bytes arrive.
    for (const row of this.rows.keys()) if (!rows.has(row)) this.baseDirtyRows.add(row)
    for (const [row, value] of rows) if (this.rows.get(row) !== value) this.baseDirtyRows.add(row)
    this.geometry = geometry
    this.rows = rows
    this.options = options
    this.visibleDirty = true
  }

  setMarkers(runs: MinimapPixelRun[]): void {
    if (runs.length === this.markers.length && runs.every((run, index) => {
      const old = this.markers[index]
      return old?.kind === run.kind && old.top === run.top && old.height === run.height
    })) return
    for (const run of [...this.markers, ...runs]) this.markerDirtyRects.push({ top: run.top, bottom: run.top + run.height })
    this.markers = runs.slice()
    this.visibleDirty = true
  }

  setViewport(box: MinimapViewportBox | null, state: 'normal' | 'hover' | 'active' = 'normal'): void {
    if (this.viewport?.top === box?.top && this.viewport?.height === box?.height &&
        this.viewport?.topPx === box?.topPx && this.viewport?.topSubPx === box?.topSubPx &&
        this.viewportState === state) return
    this.viewport = box
    this.viewportState = state
    this.visibleDirty = true
  }

  invalidateRows(rows: Iterable<bigint>): void {
    for (const row of rows) this.baseDirtyRows.add(row)
    this.visibleDirty = true
  }

  private paintRow(position: MinimapSamplePosition): void {
    const row = this.rows.get(position.row)
    if (!row) return
    const context = this.layers.base
    const colors = COLORS[this.options.theme]
    // The main editor row width is the sole source of truth for minimap columns.
    const cellWidth = (this.width - 8) / this.options.bytesPerRow
    const glyphWidth = Math.max(1, Math.min(3, Math.floor(cellWidth - 1.5)))
    const edited = new Set(row.modifiedOffsets)
    const baseOffset = position.row * BigInt(this.options.bytesPerRow)
    for (let column = 0; column < Math.min(this.options.bytesPerRow, row.bytes.length); column += 1) {
      const byte = row.bytes[column]!
      const color = edited.has(baseOffset + BigInt(column)) ? '#f0883e' : byte === 0 ? colors.zero :
        byte >= 0x20 && byte <= 0x7e ? colors.printable : colors.other
      context.fillStyle = color
      const x = Math.floor(4 + column * cellWidth)
      if (this.options.renderCharacters && position.height > 1) {
        context.font = `${Math.max(1, Math.min(position.height, cellWidth - 1))}px "JetBrains Mono", monospace`
        context.textBaseline = 'top'
        context.fillText(printable(byte), x, position.y)
      } else if (this.options.renderCharacters) {
        // Project a tiny glyph directly into a one-pixel compressed row.
        // This is not a scaled screenshot of the hex viewport.
        const glyph = printable(byte).charCodeAt(0)
        for (let bit = 0; bit < glyphWidth; bit += 1) {
          if (((glyph >> bit) & 1) !== 0) context.fillRect(x + bit, position.y, 1, 1)
        }
      } else {
        context.fillRect(x, position.y + Math.floor(position.height / 2),
          Math.max(1, Math.floor(cellWidth - 1)), 1)
      }
    }
  }

  private paintBase(): void {
    const g = this.geometry
    if (!g) return
    const context = this.layers.base
    const background = COLORS[this.options.theme].background
    const positions = samplePlan(g)
    if (this.fullBaseDirty) {
      context.clearRect(0, 0, this.width, this.height)
      context.fillStyle = background
      context.fillRect(0, 0, this.width, this.height)
      for (const position of positions) this.paintRow(position)
    } else {
      if (this.scrollShiftPx !== 0 && Math.abs(this.scrollShiftPx) < this.height) {
        const shift = this.scrollShiftPx
        const copyHeight = this.height - Math.abs(shift)
        // Reuse the existing Canvas raster cache; only newly exposed rows are painted.
        context.drawImage(context.canvas,
          0, Math.max(0, shift) * this.dpr, this.width * this.dpr, copyHeight * this.dpr,
          0, Math.max(0, -shift), this.width, copyHeight)
        const exposedTop = shift > 0 ? this.height - shift : 0
        context.clearRect(0, exposedTop, this.width, Math.abs(shift))
        context.fillStyle = background
        context.fillRect(0, exposedTop, this.width, Math.abs(shift))
        for (const position of positions) {
          if (position.y + position.height > exposedTop && position.y < exposedTop + Math.abs(shift)) this.paintRow(position)
        }
      }
      for (const position of positions) {
        if (!this.baseDirtyRows.has(position.row)) continue
        context.clearRect(0, position.y, this.width, position.height)
        context.fillStyle = background
        context.fillRect(0, position.y, this.width, position.height)
        this.paintRow(position)
      }
    }
    this.fullBaseDirty = false
    this.baseDirtyRows.clear()
    this.scrollShiftPx = 0
  }

  private paintMarkers(): void {
    const context = this.layers.markers
    const rects = this.fullMarkersDirty ? [{ top: 0, bottom: this.height }] : this.markerDirtyRects
    for (const rect of rects) {
      const top = Math.max(0, Math.min(this.height, rect.top))
      const bottom = Math.max(top, Math.min(this.height, rect.bottom))
      context.clearRect(0, top, this.width, bottom - top)
      for (const run of this.markers) {
        const overlapTop = Math.max(top, run.top)
        const overlapBottom = Math.min(bottom, run.top + run.height)
        if (overlapBottom <= overlapTop) continue
        context.fillStyle = MARKER_COLORS[run.kind]
        context.globalAlpha = run.kind === 'modified' ? 0.9 : 0.65
        context.fillRect(0, overlapTop, this.width, overlapBottom - overlapTop)
      }
    }
    context.globalAlpha = 1
    this.markerDirtyRects = []
    this.fullMarkersDirty = false
  }

  paint(): void {
    if (!this.geometry) return
    if (this.fullBaseDirty || this.baseDirtyRows.size || this.scrollShiftPx) this.paintBase()
    if (this.fullMarkersDirty || this.markerDirtyRects.length) this.paintMarkers()
    if (!this.visibleDirty) return
    const context = this.layers.visible
    context.clearRect(0, 0, this.width, this.height)
    context.drawImage(this.layers.base.canvas, 0, 0, this.width, this.height)
    context.drawImage(this.layers.markers.canvas, 0, 0, this.width, this.height)
    if (this.viewport && this.viewport.height > 0) {
      // The viewport is stored in logical canvas coordinates. Translate it
      // into the bounded bitmap only at paint time; bigint subtraction keeps
      // adjacent rows distinct even at huge file offsets.
      const top = this.geometry.mode === 'proportional'
        ? this.viewport.topSubPx !== undefined
          ? Number(this.viewport.topSubPx + canvasOffsetY(this.geometry)) / Number(MINIMAP_SUBPIXELS)
          : this.viewport.topPx !== undefined
          ? Number(this.viewport.topPx * MINIMAP_SUBPIXELS + canvasOffsetY(this.geometry)) / Number(MINIMAP_SUBPIXELS)
          : this.viewport.top + Number(canvasOffsetY(this.geometry)) / Number(MINIMAP_SUBPIXELS)
        : this.viewport.top
      const clippedTop = Math.max(0, top)
      const clippedBottom = Math.min(this.height, top + this.viewport.height)
      if (clippedBottom > clippedTop) {
        context.fillStyle = this.options.theme === 'dark' ? '#91a5b9' : '#466487'
        context.globalAlpha = this.options.theme === 'dark'
          ? this.viewportState === 'active' ? 0.25 : this.viewportState === 'hover' ? 0.21 : 0.16
          : this.viewportState === 'active' ? 0.20 : this.viewportState === 'hover' ? 0.15 : 0.1
        context.fillRect(0, clippedTop, this.width, clippedBottom - clippedTop)
        context.globalAlpha = 1
        context.strokeStyle = this.options.theme === 'dark'
          ? this.viewportState === 'active' ? '#1f6feb' : this.viewportState === 'hover' ? '#b8d4ef' : '#a0b9d2'
          : this.viewportState === 'active' ? '#0969da' : this.viewportState === 'hover' ? '#576e8a' : '#466487'
        context.strokeRect(0, clippedTop, this.width, clippedBottom - clippedTop)
      }
    }
    this.visibleDirty = false
  }

  dispose(): void {
    this.rows = new Map()
    this.markers = []
    this.geometry = null
    this.baseDirtyRows.clear()
    this.markerDirtyRects = []
  }
}
