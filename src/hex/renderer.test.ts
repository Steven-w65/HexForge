import { describe, expect, it } from 'vitest'
import { createLayout } from './layout'
import { HexRenderer, type CanvasLayerSet } from './renderer'

interface Recording {
  text: Array<{ text: string; color: string; x: number }>
  fills: string[]
  rects: Array<{ color: string; x: number; y: number; width: number; height: number }>
  fillAlphas: number[]
  strokes: string[]
  clears: number
  composites: string[]
}

function recordingContext(name: string, recording: Recording): CanvasRenderingContext2D {
  const canvas = { width: 0, height: 0, style: {}, dataset: { layer: name } } as unknown as HTMLCanvasElement
  return {
    canvas,
    fillStyle: '#000000',
    strokeStyle: '#000000',
    font: '',
    textBaseline: 'alphabetic',
    lineWidth: 1,
    globalAlpha: 1,
    setTransform: () => {},
    clearRect: () => { recording.clears += 1 },
    fillRect(this: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
      recording.fills.push(String(this.fillStyle))
      recording.fillAlphas.push(this.globalAlpha)
      recording.rects.push({ color: String(this.fillStyle), x, y, width, height })
    },
    strokeRect(this: CanvasRenderingContext2D) { recording.strokes.push(String(this.strokeStyle)) },
    fillText(this: CanvasRenderingContext2D, text: string, x: number) { recording.text.push({ text, color: String(this.fillStyle), x }) },
    drawImage: (source: CanvasImageSource) => { recording.composites.push((source as HTMLCanvasElement).dataset.layer ?? '?') },
    save: () => {},
    restore: () => {},
  } as unknown as CanvasRenderingContext2D
}

function fixture() {
  const recording: Recording = { text: [], fills: [], rects: [], fillAlphas: [], strokes: [], clears: 0, composites: [] }
  const layers: CanvasLayerSet = {
    visible: recordingContext('visible', recording),
    static: recordingContext('static', recording),
    content: recordingContext('content', recording),
    overlay: recordingContext('overlay', recording),
  }
  return { recording, layers }
}

describe('HexRenderer', () => {
  it('draws aligned column headings with separators between Offset, Hex, and ASCII', () => {
    const { recording, layers } = fixture()
    const layout = createLayout(900, 16)
    const renderer = new HexRenderer(layers, { width: 900, height: 200, dpr: 1, layout, fileSize: 64n })
    renderer.drawStatic()
    expect(recording.text.map(({ text }) => text)).toEqual(['OFFSET', 'HEX BYTES', 'ASCII'])
    const verticalDividers = recording.rects.filter((rect) => rect.color === '#30363d' && rect.width === 1 && rect.height > layout.headerHeight)
    expect(verticalDividers).toHaveLength(2)
    expect(verticalDividers[0]!.x).toBeLessThan(layout.hexX)
    expect(verticalDividers[1]!.x).toBeLessThan(layout.asciiX)
  })

  it('renders uppercase addresses and bytes while replacing non-printable ASCII with dots', () => {
    const { recording, layers } = fixture()
    const renderer = new HexRenderer(layers, {
      width: 900, height: 200, dpr: 1, layout: createLayout(900, 16), fileSize: 0x1_0000_0000n,
    })
    renderer.drawContent({ offset: '0', bytes: [0x41, 0x00, 0xaf], modifiedOffsets: [], revision: '1' }, 0n)
    const drawn = recording.text.map(({ text }) => text)
    expect(drawn).toContain('00000000')
    expect(drawn).toContain('AF')
    expect(drawn).toContain('A')
    expect(drawn.filter((text) => text === '.')).toHaveLength(2)
  })

  it.each([16, 32] as const)('aligns ASCII glyphs with selected byte cells in %i-byte rows', (bytesPerRow) => {
    const { recording, layers } = fixture()
    const layout = createLayout(1200, bytesPerRow)
    const renderer = new HexRenderer(layers, { width: 1200, height: 100, dpr: 1, layout, fileSize: 64n })
    const bytes = Array.from({ length: bytesPerRow }, (_, index) => 0x41 + index)
    renderer.drawContent({ offset: '0', bytes, modifiedOffsets: [], revision: '1' }, 0n)
    renderer.drawOverlay({
      modifiedOffsets: new Set(), selection: { start: 4n, end: 9n, count: 6n },
      matches: [], matchLength: 1, templateRange: null, viewportRow: 0n,
    })

    // A 12px monospace glyph can be narrower than the fixed 8px byte cell.
    // Expand each draw call as the browser would and compare glyph origins to highlights.
    const glyphs = recording.text
      .filter(({ x }) => x >= layout.asciiX)
      .flatMap(({ text, x }) => [...text].map((glyph, index) => ({ glyph, x: x + index * 7.2 })))
    const selectedAsciiRects = recording.rects.filter((rect) =>
      rect.color === '#1f6feb' && rect.x >= layout.asciiX,
    )
    expect(selectedAsciiRects).toHaveLength(6)
    for (let index = 4; index <= 9; index += 1) {
      expect(glyphs.find(({ glyph }) => glyph === String.fromCharCode(0x41 + index))?.x)
        .toBeCloseTo(selectedAsciiRects[index - 4]!.x)
    }
  })

  it('marks modified bytes orange and draws the three overlays with distinct treatments', () => {
    const { recording, layers } = fixture()
    const renderer = new HexRenderer(layers, {
      width: 900, height: 200, dpr: 1, layout: createLayout(900, 16), fileSize: 64n,
    })
    renderer.drawOverlay({
      modifiedOffsets: new Set(['1']),
      selection: { start: 2n, end: 3n, count: 2n },
      matches: [4n],
      matchLength: 1,
      templateRange: { start: 5n, end: 6n, count: 2n },
      viewportRow: 0n,
    })
    expect(recording.fills).toContain('#f0883e')
    expect(recording.fills).toContain('#1f6feb')
    expect(recording.fills).toContain('#8b6f2a')
    expect(recording.strokes).toContain('#39c5cf')
    const selectionFill = recording.fills.indexOf('#1f6feb')
    const matchFill = recording.fills.indexOf('#8b6f2a')
    expect(recording.fillAlphas[selectionFill]).toBeLessThan(1)
    expect(recording.fillAlphas[matchFill]).toBeLessThan(1)
    expect(recording.fills.lastIndexOf('#f0883e')).toBeGreaterThan(recording.fills.lastIndexOf('#1f6feb'))
  })

  it('highlights every byte in a multi-byte search match', () => {
    const { recording, layers } = fixture()
    const renderer = new HexRenderer(layers, {
      width: 900, height: 200, dpr: 1, layout: createLayout(900, 16), fileSize: 64n,
    })
    renderer.drawOverlay({ modifiedOffsets: new Set(), selection: null, matches: [4n], matchLength: 3, templateRange: null, viewportRow: 0n })
    expect(recording.fills.filter((color) => color === '#8b6f2a')).toHaveLength(6)
  })

  it('binary-searches sorted match starts and visits only the visible slice', () => {
    const { recording, layers } = fixture()
    const renderer = new HexRenderer(layers, {
      width: 900, height: 80, dpr: 1, layout: createLayout(900, 16), fileSize: 2_000_000n,
    })
    const values = Array.from({ length: 100_000 }, (_, index) => BigInt(index * 16))
    let reads = 0
    const matches = new Proxy(values, { get(target, property, receiver) {
      if (typeof property === 'string' && /^\d+$/.test(property)) reads += 1
      return Reflect.get(target, property, receiver)
    } })
    renderer.drawOverlay({ modifiedOffsets: new Set(), selection: null, matches, matchLength: 3, templateRange: null, viewportRow: 50_000n })

    expect(reads).toBeLessThan(64)
    expect(recording.fills.filter((color) => color === '#8b6f2a').length).toBeGreaterThan(0)
  })

  it('includes a multi-byte match that starts before but intersects the viewport', () => {
    const { recording, layers } = fixture()
    const renderer = new HexRenderer(layers, {
      width: 900, height: 80, dpr: 1, layout: createLayout(900, 16), fileSize: 1000n,
    })
    renderer.drawOverlay({ modifiedOffsets: new Set(), selection: null, matches: [14n], matchLength: 4, templateRange: null, viewportRow: 1n })
    expect(recording.fills.filter((color) => color === '#8b6f2a')).toHaveLength(4)
  })

  it('scales all backing stores and composites cached layers in order', () => {
    const { recording, layers } = fixture()
    const renderer = new HexRenderer(layers, {
      width: 320, height: 180, dpr: 2, layout: createLayout(320, 16), fileSize: 0n,
    })
    renderer.resize(400, 220, 2)
    expect(layers.visible.canvas.width).toBe(800)
    expect(layers.content.canvas.height).toBe(440)
    renderer.composite()
    expect(recording.composites).toEqual(['static', 'content', 'overlay'])
  })
})
