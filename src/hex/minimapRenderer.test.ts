import { describe, expect, it } from 'vitest'
import type { MinimapRow } from './minimapData'
import { createMinimapGeometry, viewportBox } from './minimapGeometry'
import { MinimapRenderer, type MinimapCanvasLayers } from './minimapRenderer'

interface Event { layer: string; kind: string; x?: number; y?: number; width?: number; height?: number; color?: string; text?: string }

function fixture() {
  const events: Event[] = []
  function context(layer: string): CanvasRenderingContext2D {
    return {
      canvas: { width: 0, height: 0, style: {}, dataset: { layer } },
      fillStyle: '', strokeStyle: '', globalAlpha: 1, font: '', textBaseline: 'alphabetic',
      setTransform() {},
      clearRect(x: number, y: number, width: number, height: number) { events.push({ layer, kind: 'clear', x, y, width, height }) },
      fillRect(this: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
        events.push({ layer, kind: 'fill', x, y, width, height, color: String(this.fillStyle) })
      },
      strokeRect(this: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
        events.push({ layer, kind: 'stroke', x, y, width, height, color: String(this.strokeStyle) })
      },
      fillText(this: CanvasRenderingContext2D, text: string, x: number, y: number) {
        events.push({ layer, kind: 'text', text, x, y, color: String(this.fillStyle) })
      },
      drawImage(source: CanvasImageSource) { events.push({ layer, kind: `image:${(source as HTMLCanvasElement).dataset.layer}` }) },
    } as unknown as CanvasRenderingContext2D
  }
  const layers: MinimapCanvasLayers = { visible: context('visible'), base: context('base'), markers: context('markers') }
  return { events, layers }
}

function rows(...items: Array<{ row: bigint; bytes: number[]; modified?: bigint[] }>): ReadonlyMap<bigint, MinimapRow> {
  return new Map(items.map(({ row, bytes, modified }) => [row, { row, bytes, modifiedOffsets: modified ?? [] }]))
}

const geometry = createMinimapGeometry({ mode: 'fit', totalRows: 2n, heightPx: 20, rowPx: 4, topRow: 0n })
const options = { bytesPerRow: 16 as const, renderCharacters: true, scale: 1 as const, theme: 'dark' as const }

describe('Canvas minimap renderer', () => {
  it('draws no more than the configured hex row width', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    renderer.resize(84, 8, 1)
    renderer.setContent(geometry, rows({ row: 0n, bytes: Array(32).fill(0x41) }), { ...options, renderCharacters: false })
    renderer.paint()
    expect(events.filter((event) => event.layer === 'base' && event.kind === 'fill' && event.width !== 84)).toHaveLength(16)
  })

  it('keeps compressed 32-column glyphs separated instead of overlapping adjacent bytes', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    const compressed = createMinimapGeometry({ mode: 'fit', totalRows: 100n, heightPx: 1, rowPx: 2, topRow: 0n })
    renderer.resize(84, 1, 1)
    renderer.setContent(compressed, rows({ row: 0n, bytes: Array(32).fill(0x47) }), { ...options, bytesPerRow: 32 })
    renderer.paint()
    const glyphPixels = events.filter((event) => event.layer === 'base' && event.kind === 'fill' && event.width === 1)
    expect(glyphPixels).toHaveLength(32)
    expect(new Set(glyphPixels.map((event) => event.x)).size).toBe(32)
  })

  it('draws printable characters and dots for nonprintable bytes', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    renderer.resize(84, 8, 1)
    renderer.setContent(geometry, rows({ row: 0n, bytes: [0x41, 0x00] }), options)
    renderer.paint()
    expect(events.filter((event) => event.layer === 'base' && event.kind === 'text').map((event) => event.text)).toEqual(['A', '.'])
  })

  it('uses orange for an edited byte', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    renderer.resize(84, 8, 1)
    renderer.setContent(geometry, rows({ row: 0n, bytes: [0x41, 0x42], modified: [1n] }), options)
    renderer.paint()
    expect(events.some((event) => event.layer === 'base' && event.kind === 'text' && event.text === 'B' && event.color === '#f0883e')).toBe(true)
  })

  it('highlights edited bytes in the second half of a 32-byte row', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    renderer.resize(84, 8, 1)
    renderer.setContent(geometry, rows({ row: 1n, bytes: Array(32).fill(0x41), modified: [48n] }),
      { ...options, bytesPerRow: 32 })
    renderer.paint()
    expect(events.some((event) => event.layer === 'base' && event.kind === 'text' && event.x === 42 && event.color === '#f0883e')).toBe(true)
  })

  it('composes base, markers, and then a translucent viewport box', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    renderer.resize(84, 8, 1)
    renderer.setContent(geometry, rows({ row: 0n, bytes: [0x41] }), options)
    renderer.setMarkers([{ kind: 'search', top: 0, height: 2 }])
    renderer.setViewport({ top: 0, height: 4 })
    renderer.paint()
    const visible = events.filter((event) => event.layer === 'visible')
    expect(visible.findIndex((event) => event.kind === 'image:base')).toBeLessThan(visible.findIndex((event) => event.kind === 'image:markers'))
    expect(visible.findIndex((event) => event.kind === 'image:markers')).toBeLessThan(visible.findIndex((event) => event.kind === 'fill'))
  })

  it('transforms and clips a canvas-local proportional viewport at the tile edge', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    const g = createMinimapGeometry({ mode: 'proportional', totalRows: 100n, heightPx: 8, rowPx: 2, topRow: 10n })
    renderer.resize(84, 8, 1)
    renderer.setContent(g, rows({ row: 10n, bytes: [0x41] }), options)
    renderer.setViewport({ top: 0, topSubPx: g.tileOriginSubPx + 4n * 1024n, height: 5 })
    renderer.paint()
    expect(events.find((event) => event.layer === 'visible' && event.kind === 'fill')).toMatchObject({ y: 4, height: 4 })
  })

  it('transforms a plain canvas-local proportional viewport box', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    const g = createMinimapGeometry({ mode: 'proportional', totalRows: 100n, heightPx: 8, rowPx: 2, topRow: 10n })
    renderer.resize(84, 8, 1)
    renderer.setContent(g, rows({ row: 10n, bytes: [0x41] }), options)
    renderer.setViewport({ top: 24, height: 2 })
    renderer.paint()
    expect(events.find((event) => event.layer === 'visible' && event.kind === 'fill')).toMatchObject({ y: 4, height: 2 })
  })

  it('places a huge-file proportional viewport using its exact bigint pixel origin', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    const top = (1n << 56n) + 17n
    const g = createMinimapGeometry({ mode: 'proportional', totalRows: top + 100n, heightPx: 8, rowPx: 2, topRow: top })
    renderer.resize(84, 8, 1)
    renderer.setContent(g, rows({ row: top, bytes: [0x41] }), options)
    renderer.setViewport({ top: 0, topSubPx: g.tileOriginSubPx + 6n * 1024n, height: 2 })
    renderer.paint()
    expect(events.find((event) => event.layer === 'visible' && event.kind === 'fill')).toMatchObject({ y: 6, height: 2 })
  })

  it('moves the viewport without repainting base content', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    renderer.resize(84, 8, 1)
    renderer.setContent(geometry, rows({ row: 0n, bytes: [0x41] }), options)
    renderer.paint()
    const baseClears = events.filter((event) => event.layer === 'base' && event.kind === 'clear').length
    renderer.setViewport({ top: 4, height: 4 })
    renderer.paint()
    expect(events.filter((event) => event.layer === 'base' && event.kind === 'clear')).toHaveLength(baseClears)
  })

  it('does not repaint unchanged fit content when geometry is rebuilt during scrolling', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    const data = rows({ row: 0n, bytes: [0x41] })
    renderer.resize(84, 8, 1)
    renderer.setContent(geometry, data, options)
    renderer.paint()
    events.length = 0
    const equivalent = createMinimapGeometry({ mode: 'fit', totalRows: 2n, heightPx: 20, rowPx: 4, topRow: 0n })
    renderer.setContent(equivalent, data, options)
    renderer.setViewport({ top: 4, height: 4 })
    renderer.paint()
    expect(events.filter((event) => event.layer === 'base' && event.kind === 'clear')).toHaveLength(0)
  })

  it('repaints only an invalidated row and recolors just the bounded tile on theme change', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    renderer.resize(84, 8, 1)
    const data = rows({ row: 0n, bytes: [0x41] }, { row: 1n, bytes: [0x42] })
    renderer.setContent(geometry, data, options)
    renderer.paint()
    events.length = 0
    renderer.invalidateRows([1n])
    renderer.paint()
    expect(events.filter((event) => event.layer === 'base' && event.kind === 'clear')).toEqual([
      expect.objectContaining({ y: 4, height: 4 }),
    ])
    events.length = 0
    renderer.setContent(geometry, data, { ...options, theme: 'light' })
    renderer.paint()
    expect(events.filter((event) => event.layer === 'base' && event.kind === 'clear')).toEqual([
      expect.objectContaining({ y: 0, height: 8 }),
    ])
  })

  it('clears rows removed after a source switch even when geometry is unchanged', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    renderer.resize(84, 8, 1)
    renderer.setContent(geometry, rows({ row: 0n, bytes: [0x41] }), options)
    renderer.paint()
    events.length = 0
    renderer.setContent(geometry, rows(), options)
    renderer.paint()
    expect(events.filter((event) => event.layer === 'base' && event.kind === 'clear')).toEqual([
      expect.objectContaining({ y: 0, height: 4 }),
    ])
  })

  it('reuses overlapping raster rows when a proportional minimap scrolls one row', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    const before = createMinimapGeometry({ mode: 'proportional', totalRows: 100n, heightPx: 8, rowPx: 2, topRow: 0n })
    const after = createMinimapGeometry({ mode: 'proportional', totalRows: 100n, heightPx: 8, rowPx: 2, topRow: 1n })
    const data = rows(...Array.from({ length: 5 }, (_, index) => ({ row: BigInt(index), bytes: [0x41 + index] })))
    renderer.resize(84, 8, 1)
    renderer.setContent(before, data, options)
    renderer.paint()
    events.length = 0
    renderer.setContent(after, data, options)
    renderer.paint()
    expect(events.some((event) => event.layer === 'base' && event.kind === 'image:base')).toBe(true)
    expect(events.filter((event) => event.layer === 'base' && event.kind === 'clear')).toEqual([
      expect.objectContaining({ y: 6, height: 2 }),
    ])
  })

  it('keeps a proportional shaded box aligned with its tile and strengthens its drag border', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    const g = createMinimapGeometry({ mode: 'proportional', totalRows: 1000n, heightPx: 100,
      rowPx: 2, topRow: 495n, visibleMainRows: 10n })
    renderer.resize(84, 100, 1)
    renderer.setContent(g, rows({ row: 475n, bytes: [0x41] }), options)
    renderer.setViewport(viewportBox(g, 495n, 505n), 'active')
    renderer.paint()
    expect(events.find((event) => event.layer === 'visible' && event.kind === 'fill')).toMatchObject({ y: 40, height: 20 })
    expect(events.find((event) => event.layer === 'visible' && event.kind === 'stroke')).toMatchObject({ y: 40, color: '#1f6feb' })
  })

  it('repaints a fractional proportional tile shift even when its first row is unchanged', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    const make = (visualTopPx: number) => createMinimapGeometry({ mode: 'proportional', totalRows: 1000n,
      heightPx: 101, rowPx: 2, topRow: 495n, visibleMainRows: 10n, visualTopPx })
    const first = make(40.1)
    const second = make(40.11)
    expect(first.topRow).toBe(second.topRow)
    renderer.resize(84, 101, 1)
    renderer.setContent(first, rows({ row: first.topRow, bytes: [0x41] }), options)
    renderer.paint()
    events.length = 0
    renderer.setContent(second, rows({ row: second.topRow, bytes: [0x41] }), options)
    renderer.paint()
    expect(events.some((event) => event.layer === 'base' && event.kind === 'clear' && event.height === 101)).toBe(true)
  })
})
