/** A compact 3×5 ASCII font. Each number is one row, with the left pixel in bit 2. */
const FONT: Readonly<Record<string, readonly number[]>> = {
  ' ': [0, 0, 0, 0, 0], '!': [2, 2, 2, 0, 2], '"': [5, 5, 0, 0, 0], '#': [5, 7, 5, 7, 5],
  '$': [3, 6, 2, 3, 6], '%': [5, 1, 2, 4, 5], '&': [2, 5, 2, 5, 3], "'": [2, 2, 0, 0, 0],
  '(': [1, 2, 2, 2, 1], ')': [4, 2, 2, 2, 4], '*': [0, 5, 2, 5, 0], '+': [0, 2, 7, 2, 0],
  ',': [0, 0, 0, 2, 4], '-': [0, 0, 7, 0, 0], '.': [0, 0, 0, 0, 2], '/': [1, 1, 2, 4, 4],
  '0': [7, 5, 5, 5, 7], '1': [2, 6, 2, 2, 7], '2': [6, 1, 2, 4, 7], '3': [6, 1, 2, 1, 6],
  '4': [5, 5, 7, 1, 1], '5': [7, 4, 6, 1, 6], '6': [3, 4, 6, 5, 2], '7': [7, 1, 2, 2, 2],
  '8': [2, 5, 2, 5, 2], '9': [2, 5, 3, 1, 6], ':': [0, 2, 0, 2, 0], ';': [0, 2, 0, 2, 4],
  '<': [1, 2, 4, 2, 1], '=': [0, 7, 0, 7, 0], '>': [4, 2, 1, 2, 4], '?': [6, 1, 2, 0, 2],
  '@': [2, 5, 7, 4, 3], A: [2, 5, 7, 5, 5], B: [6, 5, 6, 5, 6], C: [3, 4, 4, 4, 3],
  D: [6, 5, 5, 5, 6], E: [7, 4, 6, 4, 7], F: [7, 4, 6, 4, 4], G: [3, 4, 5, 5, 3],
  H: [5, 5, 7, 5, 5], I: [7, 2, 2, 2, 7], J: [1, 1, 1, 5, 2], K: [5, 5, 6, 5, 5],
  L: [4, 4, 4, 4, 7], M: [5, 7, 7, 5, 5], N: [5, 7, 7, 7, 5], O: [2, 5, 5, 5, 2],
  P: [6, 5, 6, 4, 4], Q: [2, 5, 5, 3, 1], R: [6, 5, 6, 5, 5], S: [3, 4, 2, 1, 6],
  T: [7, 2, 2, 2, 2], U: [5, 5, 5, 5, 7], V: [5, 5, 5, 5, 2], W: [5, 5, 7, 7, 5],
  X: [5, 5, 2, 5, 5], Y: [5, 5, 2, 2, 2], Z: [7, 1, 2, 4, 7], '[': [3, 2, 2, 2, 3],
  '\\': [4, 4, 2, 1, 1], ']': [6, 2, 2, 2, 6], '^': [2, 5, 0, 0, 0], '_': [0, 0, 0, 0, 7],
  '`': [4, 2, 0, 0, 0], a: [0, 3, 5, 5, 3], b: [4, 4, 6, 5, 6], c: [0, 3, 4, 4, 3],
  d: [1, 1, 3, 5, 3], e: [0, 2, 5, 6, 3], f: [1, 2, 7, 2, 2], g: [0, 3, 5, 3, 6],
  h: [4, 4, 6, 5, 5], i: [2, 0, 2, 2, 2], j: [1, 0, 1, 5, 2], k: [4, 5, 6, 6, 5],
  l: [6, 2, 2, 2, 3], m: [0, 5, 7, 7, 5], n: [0, 6, 5, 5, 5], o: [0, 2, 5, 5, 2],
  p: [0, 6, 5, 6, 4], q: [0, 3, 5, 3, 1], r: [0, 3, 4, 4, 4], s: [0, 3, 6, 3, 6],
  t: [2, 7, 2, 2, 3], u: [0, 5, 5, 5, 3], v: [0, 5, 5, 5, 2], w: [0, 5, 7, 7, 5],
  x: [0, 5, 2, 2, 5], y: [0, 5, 5, 3, 6], z: [0, 7, 1, 4, 7], '{': [3, 2, 6, 2, 3],
  '|': [2, 2, 2, 2, 2], '}': [6, 2, 3, 2, 6], '~': [0, 3, 6, 0, 0],
}

export interface MinimapGlyphRun { x: number; y: number; width: number }

/** Cached device-pixel stroke runs, never a document-sized bitmap or screenshot. */
export class MinimapGlyphCache {
  private readonly cache = new Map<number, readonly MinimapGlyphRun[]>()

  get(byte: number, widthPx: number, heightPx: number): readonly MinimapGlyphRun[] {
    const character = byte >= 0x20 && byte <= 0x7e ? byte : 0x2e
    // Bound both raster dimensions and cache growth even with unusual DPI.
    const width = Math.max(1, Math.min(64, Math.round(widthPx)))
    const height = Math.max(1, Math.min(64, Math.round(heightPx)))
    const key = (character * 65 + width) * 65 + height
    const cached = this.cache.get(key)
    if (cached) return cached
    const source = FONT[String.fromCharCode(character)]!
    const pixels = new Uint8Array(width * height)
    let strongest = -1
    let strongestCoverage = 0
    // Area sampling retains actual glyph strokes when several font rows
    // share one compressed minimap pixel. Thresholding keeps ink sharp,
    // unlike interpolated tiny-font text or a character-code bitmask.
    for (let y = 0; y < height; y += 1) {
      const top = y * 5 / height
      const bottom = (y + 1) * 5 / height
      for (let x = 0; x < width; x += 1) {
        const left = x * 3 / width
        const right = (x + 1) * 3 / width
        let coverage = 0
        for (let sy = Math.floor(top); sy < Math.ceil(bottom); sy += 1) {
          for (let sx = Math.floor(left); sx < Math.ceil(right); sx += 1) {
            if ((source[sy]! & (1 << (2 - sx))) === 0) continue
            coverage += (Math.min(bottom, sy + 1) - Math.max(top, sy)) * (Math.min(right, sx + 1) - Math.max(left, sx))
          }
        }
        coverage /= (bottom - top) * (right - left)
        const index = y * width + x
        if (coverage >= 0.5) pixels[index] = 1
        if (coverage > strongestCoverage) { strongest = index; strongestCoverage = coverage }
      }
    }
    // A dot/comma must survive compression. A genuine space stays empty.
    if (strongest >= 0 && !pixels.some((pixel) => pixel !== 0)) pixels[strongest] = 1
    const runs: MinimapGlyphRun[] = []
    for (let y = 0; y < height; y += 1) {
      for (let x = 0; x < width;) {
        if (!pixels[y * width + x]) { x += 1; continue }
        const start = x
        while (x < width && pixels[y * width + x]) x += 1
        runs.push({ x: start, y, width: x - start })
      }
    }
    if (this.cache.size >= 256) this.cache.delete(this.cache.keys().next().value!)
    this.cache.set(key, runs)
    return runs
  }

  clear(): void { this.cache.clear() }
}
