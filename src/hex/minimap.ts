import type { ColorTheme, ViewportPage } from '../types'
import type { BytesPerRow } from './layout'

export interface MinimapPreview {
  startRow: bigint
  rowCount: number
  contentHeight: number
}

/** Only the accepted, bounded page contributes miniature content. */
export function minimapPreview(page: ViewportPage | null, bytesPerRow: BytesPerRow, height: number): MinimapPreview | null {
  if (!page || page.bytes.length === 0 || !Number.isFinite(height) || height < 1) return null
  const rowCount = Math.ceil(page.bytes.length / bytesPerRow)
  return {
    startRow: BigInt(page.offset) / BigInt(bytesPerRow),
    rowCount,
    contentHeight: Math.min(Math.floor(height), rowCount * 2),
  }
}

export function minimapRowAt(preview: MinimapPreview, y: number): bigint {
  const clamped = Math.max(0, Math.min(preview.contentHeight - 1, Math.floor(y)))
  return preview.startRow + BigInt(Math.floor(clamped * preview.rowCount / preview.contentHeight))
}

export function minimapViewport(preview: MinimapPreview | null, scrollRow: bigint, visibleRows: number): { top: number; height: number } | null {
  if (!preview) return null
  const previewEnd = preview.startRow + BigInt(preview.rowCount)
  if (scrollRow >= previewEnd || scrollRow + BigInt(visibleRows) <= preview.startRow) return null
  const first = scrollRow <= preview.startRow ? 0 : Number(scrollRow - preview.startRow)
  const top = Math.min(preview.contentHeight, Math.floor(first * preview.contentHeight / preview.rowCount))
  const bottom = Math.min(preview.contentHeight, Math.ceil((first + visibleRows) * preview.contentHeight / preview.rowCount))
  return { top, height: Math.max(2, bottom - top) }
}

/** Tiny text-like rows from the accepted page only; the gaps avoid solid byte stripes. */
export function paintMinimap(
  context: CanvasRenderingContext2D,
  page: ViewportPage | null,
  preview: MinimapPreview | null,
  bytesPerRow: BytesPerRow,
  width: number,
  height: number,
  theme: ColorTheme,
): void {
  context.clearRect(0, 0, width, height)
  context.fillStyle = theme === 'dark' ? '#161b20' : '#f3f5f7'
  context.fillRect(0, 0, width, height)
  if (!page || !preview) return

  const colors = theme === 'dark'
    ? { zero: '#39444c', printable: '#899cac', other: '#607584' }
    : { zero: '#d8e0e6', printable: '#647b8e', other: '#9cadb9' }
  const pageStart = BigInt(page.offset)
  const modified = new Set<number>()
  for (const offset of page.modifiedOffsets) {
    const relative = BigInt(offset) - pageStart
    if (relative >= 0n && relative < BigInt(page.bytes.length)) modified.add(Number(relative))
  }
  const cellWidth = (width - 8) / bytesPerRow
  const glyphWidth = Math.max(1, Math.min(3, Math.floor(cellWidth - 1.5)))
  for (let y = 0; y < preview.contentHeight; y += 2) {
    const row = Math.floor(y * preview.rowCount / preview.contentHeight)
    for (let column = 0; column < bytesPerRow; column += 1) {
      const index = row * bytesPerRow + column
      const byte = page.bytes[index]
      if (byte === undefined) break
      const printable = byte >= 0x20 && byte <= 0x7e
      context.fillStyle = modified.has(index) ? '#f0883e' :
        byte === 0 ? colors.zero : printable ? colors.printable : colors.other
      context.fillRect(Math.floor(4 + column * cellWidth), y, byte === 0 ? 1 : printable ? glyphWidth : Math.max(1, glyphWidth - 1), 1)
    }
  }
}
