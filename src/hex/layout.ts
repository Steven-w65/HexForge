export const ROW_HEIGHT = 22
export const HEADER_HEIGHT = 32
export const MAX_READ_BYTES = 1024 * 1024
export const DEFAULT_CHAR_WIDTH = 8
export const GROUP_GAP = 8

export type BytesPerRow = 16 | 32

export interface HexLayout {
  width: number
  bytesPerRow: BytesPerRow
  rowHeight: number
  headerHeight: number
  charWidth: number
  addressX: number
  hexX: number
  asciiX: number
  byteStride: number
  groupGap: number
  byteCellWidth: number
  addressWidth: number
  hexWidth: number
  asciiWidth: number
}

export interface VisibleRange {
  firstRow: bigint
  rowCount: number
  byteStart: bigint
  byteLength: number
}

function isBytesPerRow(value: number): value is BytesPerRow {
  return value === 16 || value === 32
}

function groupsBefore(index: number): number {
  return Math.floor(index / 8)
}

export function createLayout(width: number, bytesPerRow: BytesPerRow, measuredCharWidth?: number): HexLayout {
  if (!Number.isFinite(width) || width < 0) {
    throw new RangeError('Layout width must be a non-negative finite number')
  }
  if (!isBytesPerRow(bytesPerRow)) {
    throw new RangeError('Only 16 or 32 bytes per row are supported')
  }
  const charWidth = measuredCharWidth !== undefined && Number.isFinite(measuredCharWidth) && measuredCharWidth > 0
    ? measuredCharWidth
    : DEFAULT_CHAR_WIDTH
  const addressX = charWidth * 2
  const addressWidth = charWidth * 16
  const groupGap = GROUP_GAP
  const byteStride = charWidth * 3
  const byteCellWidth = charWidth * 2
  const hexX = addressX + addressWidth + charWidth * 2
  const hexWidth = byteStride * bytesPerRow + groupsBefore(bytesPerRow - 1) * groupGap
  const asciiX = hexX + hexWidth + charWidth * 3
  const asciiWidth = charWidth * bytesPerRow + groupsBefore(bytesPerRow - 1) * groupGap
  return {
    width,
    bytesPerRow,
    rowHeight: ROW_HEIGHT,
    headerHeight: HEADER_HEIGHT,
    charWidth,
    addressX,
    hexX,
    asciiX,
    byteStride,
    groupGap,
    byteCellWidth,
    addressWidth,
    hexWidth,
    asciiWidth,
  }
}

export function contentWidth(layout: HexLayout): number {
  return Math.ceil(layout.asciiX + layout.asciiWidth + layout.charWidth * 2)
}

function cellIndexAt(layout: HexLayout, x: number, origin: number, cellWidth: number, stride: number): number | null {
  for (let index = 0; index < layout.bytesPerRow; index += 1) {
    const cellStart = origin + index * stride + groupsBefore(index) * layout.groupGap
    if (x >= cellStart && x < cellStart + cellWidth) {
      return index
    }
  }
  return null
}

export function hitTestByte(
  layout: HexLayout,
  x: number,
  y: number,
  rowOffset: bigint,
  fileSize?: bigint,
): bigint | null {
  if (!Number.isFinite(x) || !Number.isFinite(y) || y < layout.headerHeight || rowOffset < 0n) {
    return null
  }
  const row = Math.floor((y - layout.headerHeight) / layout.rowHeight)
  if (!Number.isSafeInteger(row)) {
    return null
  }
  const rowY = y - layout.headerHeight - row * layout.rowHeight
  if (rowY < 0 || rowY >= layout.rowHeight) {
    return null
  }
  const hexIndex = cellIndexAt(layout, x, layout.hexX, layout.byteCellWidth, layout.byteStride)
  const asciiIndex = cellIndexAt(layout, x, layout.asciiX, layout.charWidth, layout.charWidth)
  const index = hexIndex ?? asciiIndex
  if (index === null) {
    return null
  }
  const offset = rowOffset + BigInt(row) * BigInt(layout.bytesPerRow) + BigInt(index)
  if (fileSize !== undefined && (fileSize < 0n || offset >= fileSize)) {
    return null
  }
  return offset
}

export function visibleRange(
  layout: HexLayout,
  scrollRow: bigint,
  viewportHeight: number,
  fileSize: bigint,
): VisibleRange {
  if (fileSize < 0n) {
    throw new RangeError('File size cannot be negative')
  }
  if (!Number.isFinite(viewportHeight) || viewportHeight < 0) {
    throw new RangeError('Viewport height must be a non-negative finite number')
  }
  if (typeof scrollRow !== 'bigint') {
    throw new TypeError('Scroll row must be a bigint')
  }
  const totalRows = fileSize === 0n ? 0n : (fileSize + BigInt(layout.bytesPerRow) - 1n) / BigInt(layout.bytesPerRow)
  if (totalRows === 0n) {
    return { firstRow: 0n, rowCount: 0, byteStart: 0n, byteLength: 0 }
  }
  const prefetchRows = 2
  const maxReadRows = Math.floor(MAX_READ_BYTES / layout.bytesPerRow)
  const maxVisibleRows = Math.max(1, maxReadRows - prefetchRows * 2)
  const visibleRows = viewportHeight >= maxVisibleRows * layout.rowHeight
    ? maxVisibleRows
    : Math.max(1, Math.ceil(viewportHeight / layout.rowHeight))
  const requestedCount = Math.min(maxReadRows, visibleRows + prefetchRows * 2)
  const requestedRow = scrollRow
  const clampedRow = requestedRow < 0n ? 0n : requestedRow >= totalRows ? totalRows - 1n : requestedRow
  const firstRow = clampedRow > BigInt(prefetchRows) ? clampedRow - BigInt(prefetchRows) : 0n
  const availableRows = totalRows - firstRow
  const rowCount = Number(availableRows < BigInt(requestedCount) ? availableRows : BigInt(requestedCount))
  const byteStart = firstRow * BigInt(layout.bytesPerRow)
  const requestedBytes = BigInt(rowCount) * BigInt(layout.bytesPerRow)
  const availableBytes = fileSize - byteStart
  const byteLength = Number(
    [requestedBytes, availableBytes, BigInt(MAX_READ_BYTES)].reduce((minimum, value) => (value < minimum ? value : minimum)),
  )
  return { firstRow, rowCount, byteStart, byteLength }
}
