function asNonNegativeBigInt(value: bigint): bigint {
  return value < 0n ? 0n : value
}

function asTrackPixels(trackPx: number): bigint {
  if (!Number.isFinite(trackPx) || trackPx <= 0) {
    return 0n
  }
  return BigInt(Math.floor(trackPx))
}

/** Maps a 64-bit file position onto a viewport-sized overview without Number coercion. */
export function offsetToOverviewPixel(offset: bigint, fileSize: bigint, heightPx: number): number {
  const height = asTrackPixels(heightPx)
  if (height <= 1n || fileSize <= 1n) return 0
  const lastOffset = fileSize - 1n
  const clamped = offset <= 0n ? 0n : offset >= lastOffset ? lastOffset : offset
  return Number((clamped * (height - 1n)) / lastOffset)
}

export function rowToThumb(row: bigint, totalRows: bigint, trackPx: number): number {
  const track = asTrackPixels(trackPx)
  if (track === 0n || totalRows <= 1n) {
    return 0
  }
  const lastRow = totalRows - 1n
  const clampedRow = row <= 0n ? 0n : row >= lastRow ? lastRow : row
  return Number((clampedRow * track) / lastRow)
}

export function thumbToRow(px: number, totalRows: bigint, trackPx: number): bigint {
  const track = asTrackPixels(trackPx)
  if (track === 0n || totalRows <= 1n || !Number.isFinite(px)) {
    return 0n
  }
  const pixel = Math.max(0, Math.min(Math.floor(px), Number(track)))
  const lastRow = totalRows - 1n
  return (BigInt(pixel) * lastRow) / track
}
