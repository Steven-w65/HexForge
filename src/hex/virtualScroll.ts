function asNonNegativeBigInt(value: bigint): bigint {
  return value < 0n ? 0n : value
}

function asTrackPixels(trackPx: number): bigint {
  if (!Number.isFinite(trackPx) || trackPx <= 0) {
    return 0n
  }
  return BigInt(Math.floor(trackPx))
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
