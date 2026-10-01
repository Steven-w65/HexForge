import { describe, expect, it } from 'vitest'
import type { MinimapRow } from './minimapData'
import { createMinimapGeometry, viewportBox } from './minimapGeometry'
import { MinimapRenderer, type MinimapCanvasLayers } from './minimapRenderer'

interface Event { layer: string; kind: string; x?: number; y?: number; width?: number; height?: number; color?: string; text?: string; font?: string }

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
        events.push({ layer, kind: 'text', text, x, y, color: String(this.fillStyle), font: this.font })
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

function contrast(a: string, b: string): number {
  function luminance(hex: string): number {
    const channels = [1, 3, 5].map((start) => {
      const value = parseInt(hex.slice(start, start + 2), 16) / 255
      return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4
    })
    return channels[0]! * 0.2126 + channels[1]! * 0.7152 + channels[2]! * 0.0722
  }
  const first = luminance(a)
  const second = luminance(b)
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05)
}

function glyphRaster(events: Event[], left: number, width: number, height: number, dpr = 1): string[] {
  const ink = events.filter((event) => event.layer === 'base' && event.kind === 'fill' && event.width! < 80)
  return Array.from({ length: height }, (_, y) => Array.from({ length: width }, (_, x) => {
    const px = left + (x + 0.5) / dpr
    const py = (y + 0.5) / dpr
    return ink.some((event) => px >= event.x! && px < event.x! + event.width! && py >= event.y! && py < event.y! + event.height!) ? '1' : '0'
  }).join(''))
}

describe('Canvas minimap renderer', () => {
  it.each([16, 32] as const)('renders distinct compact glyphs instead of uniform boxes in %i-byte rows', (bytesPerRow) => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    const compact = createMinimapGeometry({ mode: 'proportional', totalRows: 1n, heightPx: 20, rowPx: 2, topRow: 0n })
    const width = bytesPerRow === 32 ? 136 : 84
    renderer.resize(width, 2, 1)
    renderer.setContent(compact, rows({ row: 0n, bytes: [0x41, 0x42, 0x20, 0] }),
      { ...options, bytesPerRow })
    renderer.paint()
    const stride = (width - 8) / bytesPerRow
    expect(glyphRaster(events, 4, 3, 2)).toEqual(['111', '101'])
    expect(glyphRaster(events, Math.round(4 + stride), 3, 2)).toEqual(['110', '110'])
    expect(glyphRaster(events, Math.round(4 + 2 * stride), 3, 2)).toEqual(['000', '000'])
    expect(glyphRaster(events, Math.round(4 + 3 * stride), 3, 2)).toEqual(['000', '010'])
    expect(events.some((event) => event.layer === 'base' && event.kind === 'text')).toBe(false)
  })

  it('does not lose even-valued ASCII bytes in compressed character rows', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    const compressed = createMinimapGeometry({ mode: 'fit', totalRows: 100n, heightPx: 1, rowPx: 2, topRow: 0n })
    renderer.resize(84, 1, 1)
    renderer.setContent(compressed, rows({ row: 0n, bytes: Array(32).fill(0x42) }), { ...options, bytesPerRow: 32 })
    renderer.paint()
    expect(events.filter((event) => event.layer === 'base' && event.kind === 'fill' && event.width !== 84)).toHaveLength(32)
  })

  it.each(['dark', 'light'] as const)('keeps zero, printable, and binary byte strokes distinguishable from the %s background', (theme) => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    renderer.resize(84, 8, 1)
    renderer.setContent(geometry, rows({ row: 0n, bytes: [0, 0x41, 0xff] }), { ...options, theme, renderCharacters: false })
    renderer.paint()
    const fills = events.filter((event) => event.layer === 'base' && event.kind === 'fill')
    const background = fills[0]!.color!
    const ink = fills.slice(1)
    expect(ink).toHaveLength(3)
    expect(new Set(ink.map((event) => event.color)).size).toBe(3)
    for (const event of ink) expect(contrast(event.color!, background)).toBeGreaterThanOrEqual(3)
  })

  it('renders a sharp A outline at the larger scale rather than a font call or solid box', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    const large = createMinimapGeometry({ mode: 'fit', totalRows: 1n, heightPx: 20, rowPx: 6, topRow: 0n })
    renderer.resize(84, 6, 1)
    renderer.setContent(large, rows({ row: 0n, bytes: [0x41] }), { ...options, scale: 3 })
    renderer.paint()
    expect(glyphRaster(events, 4, 3, 6)).toEqual(['010', '101', '111', '101', '101', '000'])
    expect(events.some((event) => event.layer === 'base' && event.kind === 'text')).toBe(false)
  })

  it.each([1.25, 2])('aligns glyph strokes to physical pixels at DPR %s', (dpr) => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    renderer.resize(84, 8, dpr)
    renderer.setContent(geometry, rows({ row: 0n, bytes: [0x41, 0x42] }), options)
    renderer.paint()
    const ink = events.filter((event) => event.layer === 'base' && event.kind === 'fill' && event.width! < 80)
    expect(ink.length).toBeGreaterThan(2)
    for (const event of ink) {
      for (const value of [event.x!, event.y!, event.width!, event.height!]) expect(value * dpr).toBeCloseTo(Math.round(value * dpr), 8)
    }
    expect(layers.base.canvas.width).toBe(Math.round(84 * dpr))
    expect(layers.base.canvas.height).toBe(Math.round(8 * dpr))
  })

  it.each(['fit', 'proportional'] as const)('retains dots in every %s sample at fractional display scaling', (mode) => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    const g = createMinimapGeometry({ mode, totalRows: mode === 'fit' ? 100n : 4n, heightPx: 8, rowPx: 2, topRow: 0n })
    renderer.resize(84, 8, 1.25)
    renderer.setContent(g, rows(...Array.from({ length: 100 }, (_, index) => ({ row: BigInt(index), bytes: [0] }))), options)
    renderer.paint()
    const ink = events.filter((event) => event.layer === 'base' && event.kind === 'fill' && event.width! < 80)
    // Natural rows get alternately 3/2 device pixels; compressed rows get
    // alternately 1/2. Neither may crop away its punctuation glyph.
    expect(ink).toHaveLength(mode === 'fit' ? 8 : 4)
  })

  it('clears a changed glyph on device-pixel row boundaries at fractional DPI', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    const g = createMinimapGeometry({ mode: 'proportional', totalRows: 4n, heightPx: 8, rowPx: 2, topRow: 0n })
    renderer.resize(84, 8, 1.25)
    renderer.setContent(g, rows({ row: 0n, bytes: [0x41] }, { row: 1n, bytes: [0x42] }), options)
    renderer.paint()
    events.length = 0
    renderer.setContent(g, rows({ row: 0n, bytes: [0x20] }, { row: 1n, bytes: [0x42] }), options)
    renderer.paint()
    const clears = events.filter((event) => event.layer === 'base' && event.kind === 'clear')
    expect(clears[0]).toMatchObject({ y: 0, height: 2.4 })
    for (const event of clears) {
      expect(event.y! * 1.25).toBeCloseTo(Math.round(event.y! * 1.25), 8)
      expect(event.height! * 1.25).toBeCloseTo(Math.round(event.height! * 1.25), 8)
    }
  })

  it('retains an orange glyph for an edited space while ordinary spaces stay blank', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    renderer.resize(84, 8, 1)
    renderer.setContent(geometry, rows({ row: 0n, bytes: [0x20, 0x20], modified: [1n] }), options)
    renderer.paint()
    expect(glyphRaster(events, 4, 3, 4)).toEqual(['000', '000', '000', '000'])
    expect(events.some((event) => event.layer === 'base' && event.kind === 'fill' && event.color === '#f0883e' && event.x! >= 9 && event.x! < 12)).toBe(true)
  })

  it('gives larger color-block rows thicker ink without filling their separating row gap', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    const large = createMinimapGeometry({ mode: 'proportional', totalRows: 1n, heightPx: 20, rowPx: 6, topRow: 0n })
    renderer.resize(84, 6, 1)
    renderer.setContent(large, rows({ row: 0n, bytes: [0x41] }), { ...options, scale: 3, renderCharacters: false })
    renderer.paint()
    const ink = events.find((event) => event.layer === 'base' && event.kind === 'fill' && event.width !== 84)!
    expect(ink.height).toBeGreaterThanOrEqual(2)
    expect(ink.y! + ink.height!).toBeLessThan(6)
  })

  it('bounds compact byte ink to fractional rows at both proportional tile edges', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    // Logical height 12px, travel 8px: a 2px box top gives a 0.5px
    // virtual origin, leaving a 1.5px first row and a 0.5px final row.
    const partial = createMinimapGeometry({ mode: 'proportional', totalRows: 6n, heightPx: 10, rowPx: 2,
      topRow: 1n, visibleMainRows: 1n, visualTopPx: 2 })
    renderer.resize(84, 10, 2)
    renderer.setContent(partial, rows({ row: 0n, bytes: [0x41] }, { row: 5n, bytes: [0x42] }), options)
    renderer.paint()
    const ink = events.filter((event) => event.layer === 'base' && event.kind === 'fill' && event.width !== 84)
    expect(ink.some((event) => event.y! < 1.5)).toBe(true)
    expect(ink.some((event) => event.y! >= 9.5)).toBe(true)
    expect(ink.every((event) => event.y! >= 0 && event.y! + event.height! <= 10)).toBe(true)
    expect(ink.every((event) => event.y! + event.height! <= 1.5 || event.y! >= 9.5)).toBe(true)
  })

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
    const glyphPixels = events.filter((event) => event.layer === 'base' && event.kind === 'fill' && event.width !== 84)
    expect(glyphPixels).toHaveLength(32)
    expect(new Set(glyphPixels.map((event) => event.x)).size).toBe(32)
  })

  it('draws printable characters and dots for nonprintable bytes', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    renderer.resize(84, 8, 1)
    renderer.setContent(geometry, rows({ row: 0n, bytes: [0x41, 0x00] }), options)
    renderer.paint()
    expect(glyphRaster(events, 4, 3, 4)).toEqual(['010', '101', '101', '101'])
    expect(glyphRaster(events, 9, 3, 4)).toEqual(['000', '000', '000', '010'])
  })

  it('uses orange for an edited byte', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    renderer.resize(84, 8, 1)
    renderer.setContent(geometry, rows({ row: 0n, bytes: [0x41, 0x42], modified: [1n] }), options)
    renderer.paint()
    const editedGlyph = events.filter((event) => event.layer === 'base' && event.kind === 'fill' && event.color === '#f0883e')
    expect(editedGlyph.length).toBeGreaterThan(1)
    expect(editedGlyph.every((event) => event.x! >= 9 && event.x! + event.width! <= 12)).toBe(true)
  })

  it('highlights edited bytes in the second half of a 32-byte row', () => {
    const { events, layers } = fixture()
    const renderer = new MinimapRenderer(layers)
    renderer.resize(84, 8, 1)
    renderer.setContent(geometry, rows({ row: 1n, bytes: Array(32).fill(0x41), modified: [48n] }),
      { ...options, bytesPerRow: 32 })
    renderer.paint()
    expect(events.some((event) => event.layer === 'base' && event.kind === 'fill' && event.x === 42 && event.color === '#f0883e')).toBe(true)
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
