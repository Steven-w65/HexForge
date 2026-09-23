import type { PageResponse } from '../types'
import type { ByteSelection } from './selection'
import type { HexLayout } from './layout'

export interface CanvasLayerSet {
  visible: CanvasRenderingContext2D
  static: CanvasRenderingContext2D
  content: CanvasRenderingContext2D
  overlay: CanvasRenderingContext2D
}

export interface RendererMetrics {
  width: number
  height: number
  dpr: number
  layout: HexLayout
  fileSize: bigint
}

export interface RendererTheme {
  background: string
  text: string
  address: string
  divider: string
}

export interface OverlayState {
  modifiedOffsets: ReadonlySet<string>
  selection: ByteSelection | null
  matches: readonly bigint[]
  matchLength: number
  templateRange: ByteSelection | null
  viewportRow: bigint
}

const DEFAULT_THEME: RendererTheme = {
  background: '#111418',
  text: '#c9d1d9',
  address: '#8b949e',
  divider: '#30363d',
}

const SELECTION = '#1f6feb'
const MATCH = '#8b6f2a'
const TEMPLATE = '#39c5cf'
const MODIFIED = '#f0883e'

function makeOffscreenContext(): CanvasRenderingContext2D {
  const canvas = document.createElement('canvas')
  const context = canvas.getContext('2d')
  if (!context) throw new Error('Canvas 2D rendering is unavailable.')
  return context
}

export function createCanvasLayers(canvas: HTMLCanvasElement): CanvasLayerSet {
  const visible = canvas.getContext('2d')
  if (!visible) throw new Error('Canvas 2D rendering is unavailable.')
  return { visible, static: makeOffscreenContext(), content: makeOffscreenContext(), overlay: makeOffscreenContext() }
}

function addressDigits(fileSize: bigint): number {
  const largest = fileSize > 0n ? fileSize - 1n : 0n
  return Math.max(8, largest.toString(16).length)
}

function printable(value: number): string {
  return value >= 0x20 && value <= 0x7e ? String.fromCharCode(value) : '.'
}

function groupGap(index: number): number {
  return Math.floor(index / 8)
}

export class HexRenderer {
  private theme: RendererTheme = { ...DEFAULT_THEME }
  private metrics: RendererMetrics
  private viewportRow = 0n

  constructor(private readonly layers: CanvasLayerSet, metrics: RendererMetrics) {
    this.metrics = metrics
    this.resize(metrics.width, metrics.height, metrics.dpr)
  }

  resize(width: number, height: number, dpr = globalThis.devicePixelRatio || 1): void {
    const safeDpr = Number.isFinite(dpr) && dpr > 0 ? dpr : 1
    this.metrics = { ...this.metrics, width, height, dpr: safeDpr }
    for (const context of Object.values(this.layers)) {
      context.canvas.width = Math.max(1, Math.round(width * safeDpr))
      context.canvas.height = Math.max(1, Math.round(height * safeDpr))
      context.canvas.style.width = `${width}px`
      context.canvas.style.height = `${height}px`
      context.setTransform(safeDpr, 0, 0, safeDpr, 0, 0)
    }
  }

  setTheme(theme: Partial<RendererTheme>): void {
    this.theme = { ...this.theme, ...theme }
  }

  setViewport(layout: HexLayout, fileSize: bigint, viewportRow: bigint): void {
    this.metrics = { ...this.metrics, layout, fileSize }
    this.viewportRow = viewportRow
  }

  drawStatic(): void {
    const { static: context } = this.layers
    const { width, height, layout } = this.metrics
    context.clearRect(0, 0, width, height)
    context.fillStyle = this.theme.background
    context.fillRect(0, 0, width, height)
    context.font = `${layout.charWidth * 1.5}px "JetBrains Mono", monospace`
    context.textBaseline = 'middle'
    context.fillStyle = this.theme.address
    context.fillText('OFFSET', layout.addressX, layout.headerHeight / 2)
    context.fillText('HEX BYTES', layout.hexX, layout.headerHeight / 2)
    context.fillText('ASCII', layout.asciiX, layout.headerHeight / 2)
    context.fillStyle = this.theme.divider
    context.fillRect(0, layout.headerHeight - 1, width, 1)
    const addressDividerX = layout.hexX - layout.charWidth
    const hexDividerX = layout.asciiX - layout.charWidth * 1.5
    context.fillRect(addressDividerX, 0, 1, height)
    context.fillRect(hexDividerX, 0, 1, height)
  }

  drawContent(page: PageResponse | null, viewportRow = this.viewportRow): void {
    const { content: context } = this.layers
    const { width, height, layout, fileSize } = this.metrics
    context.clearRect(0, 0, width, height)
    if (!page) return
    context.font = `${layout.charWidth * 1.5}px "JetBrains Mono", monospace`
    context.textBaseline = 'middle'
    const pageOffset = BigInt(page.offset)
    const firstPageRow = pageOffset / BigInt(layout.bytesPerRow)
    const prefix = Number(pageOffset % BigInt(layout.bytesPerRow))
    const rows = new Map<bigint, Array<number | undefined>>()
    page.bytes.forEach((value, index) => {
      const position = prefix + index
      const row = firstPageRow + BigInt(Math.floor(position / layout.bytesPerRow))
      const column = position % layout.bytesPerRow
      const values = rows.get(row) ?? Array<number | undefined>(layout.bytesPerRow)
      values[column] = value
      rows.set(row, values)
    })
    const digits = addressDigits(fileSize)
    for (const [row, values] of rows) {
      const relative = row - viewportRow
      if (relative < 0n || relative > BigInt(Number.MAX_SAFE_INTEGER)) continue
      const y = layout.headerHeight + Number(relative) * layout.rowHeight + layout.rowHeight / 2
      if (y < layout.headerHeight || y > height) continue
      const rowOffset = row * BigInt(layout.bytesPerRow)
      context.fillStyle = this.theme.address
      context.fillText(rowOffset.toString(16).toUpperCase().padStart(digits, '0'), layout.addressX, y)
      context.fillStyle = this.theme.text
      const asciiGroups: string[] = []
      for (let groupStart = 0; groupStart < layout.bytesPerRow; groupStart += 8) {
        let ascii = ''
        for (let column = groupStart; column < Math.min(groupStart + 8, layout.bytesPerRow); column += 1) {
          const value = values[column]
          if (value === undefined) continue
          const x = layout.hexX + column * layout.byteStride + groupGap(column) * layout.groupGap
          context.fillText(value.toString(16).toUpperCase().padStart(2, '0'), x, y)
          ascii += printable(value)
        }
        asciiGroups.push(ascii)
      }
      asciiGroups.forEach((ascii, group) => {
        if (ascii) context.fillText(ascii, layout.asciiX + group * (8 * layout.charWidth + layout.groupGap), y)
      })
    }
  }

  drawOverlay(state: OverlayState): void {
    const { overlay: context } = this.layers
    const { width, height, layout } = this.metrics
    this.viewportRow = state.viewportRow
    context.clearRect(0, 0, width, height)
    const drawFill = (offset: bigint, color: string, alpha = 1) => {
      const cell = this.cell(offset, state.viewportRow)
      if (!cell) return
      context.fillStyle = color
      context.globalAlpha = alpha
      context.fillRect(cell.hexX, cell.y, layout.byteCellWidth, layout.rowHeight)
      context.fillRect(cell.asciiX, cell.y, layout.charWidth, layout.rowHeight)
      context.globalAlpha = 1
    }
    const drawRange = (range: ByteSelection | null, color: string, stroke = false, alpha = 0.48) => {
      if (!range) return
      const firstVisible = state.viewportRow * BigInt(layout.bytesPerRow)
      const visibleRows = BigInt(Math.ceil(Math.max(0, height - layout.headerHeight) / layout.rowHeight))
      const lastVisible = firstVisible + visibleRows * BigInt(layout.bytesPerRow) - 1n
      const start = range.start > firstVisible ? range.start : firstVisible
      const end = range.end < lastVisible ? range.end : lastVisible
      if (end < start) return
      for (let offset = start; offset <= end; offset += 1n) {
        if (stroke) {
          const cell = this.cell(offset, state.viewportRow)
          if (!cell) continue
          context.strokeStyle = color
          context.strokeRect(cell.hexX, cell.y, layout.byteCellWidth, layout.rowHeight)
          context.strokeRect(cell.asciiX, cell.y, layout.charWidth, layout.rowHeight)
        } else drawFill(offset, color, alpha)
      }
    }
    const matchLength = Number.isSafeInteger(state.matchLength) && state.matchLength > 0 ? state.matchLength : 1
    const count = BigInt(matchLength)
    const firstVisible = state.viewportRow * BigInt(layout.bytesPerRow)
    const visibleRows = BigInt(Math.ceil(Math.max(0, height - layout.headerHeight) / layout.rowHeight))
    const lastVisible = firstVisible + visibleRows * BigInt(layout.bytesPerRow) - 1n
    const earliestIntersectingStart = firstVisible >= count - 1n ? firstVisible - count + 1n : 0n
    let low = 0
    let high = state.matches.length
    while (low < high) {
      const middle = low + Math.floor((high - low) / 2)
      if (state.matches[middle]! < earliestIntersectingStart) low = middle + 1
      else high = middle
    }
    for (const offsetValue of state.modifiedOffsets) drawFill(BigInt(offsetValue), MODIFIED, 0.22)
    for (let index = low; index < state.matches.length; index += 1) {
      const start = state.matches[index]!
      if (start > lastVisible) break
      drawRange({ start, end: start + count - 1n, count }, MATCH, false, 0.42)
    }
    drawRange(state.selection, SELECTION)
    drawRange(state.templateRange, TEMPLATE, true)
    for (const offsetValue of state.modifiedOffsets) {
      const cell = this.cell(BigInt(offsetValue), state.viewportRow)
      if (!cell) continue
      context.fillStyle = MODIFIED
      context.globalAlpha = 1
      context.fillRect(cell.hexX, cell.y + layout.rowHeight - 2, layout.byteCellWidth, 2)
      context.fillRect(cell.asciiX, cell.y + layout.rowHeight - 2, layout.charWidth, 2)
    }
  }

  composite(): void {
    const { visible, static: staticLayer, content, overlay } = this.layers
    const { width, height, dpr } = this.metrics
    visible.clearRect(0, 0, width, height)
    visible.drawImage(staticLayer.canvas, 0, 0, width * dpr, height * dpr, 0, 0, width, height)
    visible.drawImage(content.canvas, 0, 0, width * dpr, height * dpr, 0, 0, width, height)
    visible.drawImage(overlay.canvas, 0, 0, width * dpr, height * dpr, 0, 0, width, height)
  }

  private cell(offset: bigint, viewportRow: bigint) {
    const { layout, height } = this.metrics
    if (offset < 0n) return null
    const row = offset / BigInt(layout.bytesPerRow)
    const relativeRow = row - viewportRow
    if (relativeRow < 0n || relativeRow > BigInt(Number.MAX_SAFE_INTEGER)) return null
    const column = Number(offset % BigInt(layout.bytesPerRow))
    const y = layout.headerHeight + Number(relativeRow) * layout.rowHeight
    if (y >= height) return null
    return {
      y,
      hexX: layout.hexX + column * layout.byteStride + groupGap(column) * layout.groupGap,
      asciiX: layout.asciiX + column * layout.charWidth + groupGap(column) * layout.groupGap,
    }
  }
}
