import { describe, expect, it } from 'vitest'
import { createLayout } from './layout'
import { HexRenderer, type CanvasLayerSet } from './renderer'

interface Recording {
  text: Array<{ text: string; color: string }>
  fills: string[]
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
    fillRect(this: CanvasRenderingContext2D) {
      recording.fills.push(String(this.fillStyle))
      recording.fillAlphas.push(this.globalAlpha)
    },
    strokeRect(this: CanvasRenderingContext2D) { recording.strokes.push(String(this.strokeStyle)) },
    fillText(this: CanvasRenderingContext2D, text: string) { recording.text.push({ text, color: String(this.fillStyle) }) },
    drawImage: (source: CanvasImageSource) => { recording.composites.push((source as HTMLCanvasElement).dataset.layer ?? '?') },
    save: () => {},
    restore: () => {},
  } as unknown as CanvasRenderingContext2D
}

function fixture() {
  const recording: Recording = { text: [], fills: [], fillAlphas: [], strokes: [], clears: 0, composites: [] }
  const layers: CanvasLayerSet = {
    visible: recordingContext('visible', recording),
    static: recordingContext('static', recording),
    content: recordingContext('content', recording),
    overlay: recordingContext('overlay', recording),
  }
  return { recording, layers }
}

describe('HexRenderer', () => {
  it('renders uppercase addresses and bytes while replacing non-printable ASCII with dots', () => {
    const { recording, layers } = fixture()
    const renderer = new HexRenderer(layers, {
      width: 900, height: 200, dpr: 1, layout: createLayout(900, 16), fileSize: 0x1_0000_0000n,
    })
    renderer.drawContent({ offset: '0', bytes: [0x41, 0x00, 0xaf], modifiedOffsets: [], revision: '1' }, 0n)
    const drawn = recording.text.map(({ text }) => text)
    expect(drawn).toContain('00000000')
    expect(drawn).toContain('AF')
    expect(drawn).toContain('A..')
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
  })

  it('highlights every byte in a multi-byte search match', () => {
    const { recording, layers } = fixture()
    const renderer = new HexRenderer(layers, {
      width: 900, height: 200, dpr: 1, layout: createLayout(900, 16), fileSize: 64n,
    })
    renderer.drawOverlay({ modifiedOffsets: new Set(), selection: null, matches: [4n], matchLength: 3, templateRange: null, viewportRow: 0n })
    expect(recording.fills.filter((color) => color === '#8b6f2a')).toHaveLength(6)
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
