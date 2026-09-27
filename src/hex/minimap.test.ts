import { describe, expect, it } from 'vitest'
import { minimapPreview, minimapRowAt, minimapViewport } from './minimap'

describe('hex minimap', () => {
  it('does not create a zero-height preview while the viewport is collapsing', () => {
    const page = { offset: '0', bytes: [0x41], modifiedOffsets: [], revision: '1', generation: 1 }
    expect(minimapPreview(page, 16, 0.5)).toBeNull()
  })

  it('maps nearby preview clicks without losing 64-bit offsets', () => {
    const start = 1n << 56n
    const page = { offset: start.toString(), bytes: Array(16 * 100).fill(0x41), modifiedOffsets: [], revision: '1', generation: 1 }
    const preview = minimapPreview(page, 16, 500)!
    expect(minimapRowAt(preview, 100)).toBe(start / 16n + 50n)
  })

  it('hides the old viewport outline after a full-file jump leaves the cached preview', () => {
    const page = { offset: '0', bytes: Array(16 * 100).fill(0x41), modifiedOffsets: [], revision: '1', generation: 1 }
    const preview = minimapPreview(page, 16, 500)!
    expect(minimapViewport(preview, 10_000n, 20)).toBeNull()
  })
})
