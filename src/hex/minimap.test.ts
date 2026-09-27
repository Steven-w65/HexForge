import { describe, expect, it } from 'vitest'
import { minimapPreview, minimapRowAt, minimapViewport, paintMinimap } from './minimap'

function recordedCanvas() {
  const fills: Array<{ x: number; y: number; width: number; height: number; color: string }> = []
  const context = {
    fillStyle: '',
    clearRect() {},
    fillRect(this: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
      fills.push({ x, y, width, height, color: String(this.fillStyle) })
    },
  } as unknown as CanvasRenderingContext2D
  return { context, fills }
}

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

  it('paints separated miniature rows instead of continuous vertical byte stripes', () => {
    const page = { offset: '0', bytes: [...Array(16).fill(0x41), ...Array(16).fill(0x42)], modifiedOffsets: [], revision: '1', generation: 1 }
    const preview = minimapPreview(page, 16, 4)!
    const { context, fills } = recordedCanvas()

    paintMinimap(context, page, preview, 16, 84, 4, 'dark')

    const marks = fills.filter((fill) => fill.width < 84)
    expect([...new Set(marks.map((mark) => mark.y))]).toEqual([0, 2])
    expect(marks.every((mark) => mark.height === 1)).toBe(true)
  })

  it('leaves a visible gap between miniature glyphs in a 16-byte row', () => {
    const page = { offset: '0', bytes: Array(16).fill(0x41), modifiedOffsets: [], revision: '1', generation: 1 }
    const preview = minimapPreview(page, 16, 2)!
    const { context, fills } = recordedCanvas()

    paintMinimap(context, page, preview, 16, 84, 2, 'dark')

    const marks = fills.filter((fill) => fill.width < 84 && fill.y === 0)
    expect(marks).toHaveLength(16)
    expect(marks.every((mark, index) => index === marks.length - 1 || mark.x + mark.width < marks[index + 1]!.x)).toBe(true)
  })
})
