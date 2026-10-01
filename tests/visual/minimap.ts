import { createApp, h, nextTick, ref } from 'vue'
import { createMinimapGeometry, samplePlan, viewportBox, type MinimapMode } from '../../src/hex/minimapGeometry'
import { createMinimapCanvasLayers, MinimapRenderer } from '../../src/hex/minimapRenderer'
import type { MinimapRow } from '../../src/hex/minimapData'
import type { BytesPerRow } from '../../src/hex/layout'
import { backend } from '../../src/api/backend'
import HexCanvas from '../../src/components/HexCanvas.vue'
import TemplateEditorWindow from '../../src/components/TemplateEditorWindow.vue'
import type { PageRequest, ViewportPage } from '../../src/types'

interface RendererResult {
  bottomRowPainted: boolean
  lastColumnPainted: boolean
  boxBorderPainted: boolean
  backingHeight: number
}

interface ComponentResult {
  addressGutterPainted: boolean
  hexEndsBeforeMinimap: boolean
  minimapFillsHeight: boolean
  previewHeight: number
  minimapHeight: number
}

interface DialogButtonResult {
  borderStyle: string
  borderRadius: string
  height: string
  disabledOpacity: string
}

interface ReadabilityResult {
  mode: MinimapMode
  bytesPerRow: BytesPerRow
  theme: 'dark' | 'light'
  minimumByteContrast: number
  allColumnsPainted: boolean
  dpr: number
  sharpPixels: boolean
  distinctCharacters: boolean
}

declare global {
  interface Window {
    __hexforgeVisualResults?: {
      fit16: RendererResult
      fit32: RendererResult
      proportional16: RendererResult
      proportional32: RendererResult
      component16Fit: ComponentResult
      component32Proportional: ComponentResult
      templateDialogButtons: DialogButtonResult
      readability: ReadabilityResult[]
    }
    __hexforgeVisualError?: string
  }
}

const background = [0x16, 0x1b, 0x20]
const orange = [0xf0, 0x88, 0x3e]

function renderTemplateDialogButtons(): DialogButtonResult {
  // Import the real SFC stylesheet, then render its scoped action markup in
  // the browser. This catches native button borders that jsdom cannot see.
  const scopeId = (TemplateEditorWindow as typeof TemplateEditorWindow & { __scopeId?: string }).__scopeId
  if (!scopeId) throw new Error('Template Editor scoped styles were not loaded.')
  const actions = document.createElement('div')
  actions.className = 'close-actions'
  actions.setAttribute(scopeId, '')
  const saveAs = document.createElement('button')
  saveAs.textContent = 'Save As'
  saveAs.setAttribute(scopeId, '')
  const save = document.createElement('button')
  save.textContent = 'Save'
  save.disabled = true
  save.setAttribute(scopeId, '')
  actions.append(saveAs, save)
  document.body.append(actions)
  try {
    const normalStyle = getComputedStyle(saveAs)
    return {
      borderStyle: normalStyle.borderStyle,
      borderRadius: normalStyle.borderRadius,
      height: normalStyle.height,
      disabledOpacity: getComputedStyle(save).opacity,
    }
  } finally { actions.remove() }
}

function pixel(context: CanvasRenderingContext2D, x: number, y: number): number[] {
  return [...context.getImageData(x, y, 1, 1).data]
}

function differs(left: number[], right: number[]): boolean {
  return left.some((value, index) => value !== right[index])
}

function pixelContrast(a: number[], b: number[]): number {
  const luminance = (rgb: number[]) => rgb.slice(0, 3).reduce((sum, channel, index) => {
    const value = channel / 255
    const linear = value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4
    return sum + linear * [0.2126, 0.7152, 0.0722][index]!
  }, 0)
  const first = luminance(a)
  const second = luminance(b)
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05)
}

function renderReadability(mode: MinimapMode, bytesPerRow: BytesPerRow, theme: 'dark' | 'light', dpr: number): ReadabilityResult {
  const canvas = document.createElement('canvas')
  const layers = createMinimapCanvasLayers(canvas)
  const renderer = new MinimapRenderer(layers)
  // 102 CSS pixels has a fractional physical height at 125% scaling.
  // Include that rounded backing edge, not only conveniently integral sizes.
  const geometry = createMinimapGeometry({ mode, totalRows: 1000n, heightPx: 102, rowPx: 2, topRow: 0n, visibleMainRows: 10n })
  const rows = new Map<bigint, MinimapRow>()
  const bytes = Array.from({ length: bytesPerRow }, (_, index) => [0xff, 0, 0x41, 0x42, 0x43, 0x2e][index % 6]!)
  for (const position of samplePlan(geometry)) rows.set(position.row, { row: position.row, bytes, modifiedOffsets: [] })
  const width = bytesPerRow === 32 ? 136 : 84
  renderer.resize(width, 102, dpr)
  renderer.setContent(geometry, rows, { bytesPerRow, renderCharacters: true, scale: 1, theme })
  renderer.paint()
  const backgroundPixel = pixel(layers.base, 0, 0)
  const cellWidth = (width - 8) / bytesPerRow
  const contrasts: number[] = []
  const signatures: string[] = []
  // Inspect the actual raster, not fillText calls: tiny fonts can request a
  // bright color yet leave almost no visible ink after antialiasing.
  for (let column = 0; column < bytesPerRow; column += 1) {
    let maximum = 1
    let signature = ''
    const left = Math.round((4 + column * cellWidth) * dpr)
    for (let y = 0; y < Math.round((mode === 'fit' ? 1 : 2) * dpr); y += 1) {
      for (let x = left; x < left + Math.round(3 * dpr); x += 1) {
        const contrast = pixelContrast(pixel(layers.base, x, y), backgroundPixel)
        maximum = Math.max(maximum, contrast)
        signature += contrast > 1 ? '1' : '0'
      }
    }
    signatures.push(signature)
    contrasts.push(maximum)
  }
  // Only exact background or palette ink is allowed in the base raster;
  // intermediate shades indicate antialiased/blurry glyph edges.
  const palette = theme === 'dark' ? ['161b20', '5f7485', 'aec0d0', '8ca2b5'] : ['f3f5f7', '7f8e9b', '425f77', '637d92']
  let sharpPixels = true
  for (const context of [layers.base, layers.visible]) {
    const data = context.getImageData(0, 0, context.canvas.width, context.canvas.height).data
    for (let i = 0; i < data.length; i += 4) {
      const rgb = [...data.slice(i, i + 3)].map((value) => value.toString(16).padStart(2, '0')).join('')
      if (data[i + 3] !== 255 || !palette.includes(rgb)) { sharpPixels = false; break }
    }
  }
  renderer.dispose()
  return { mode, bytesPerRow, theme, dpr, sharpPixels, distinctCharacters: new Set(signatures.slice(2, 5)).size === 3,
    minimumByteContrast: Math.min(...contrasts), allColumnsPainted: contrasts.every((value) => value > 1) }
}

function render(mode: MinimapMode, bytesPerRow: BytesPerRow) {
  const canvas = document.createElement('canvas')
  document.body.append(canvas)
  const layers = createMinimapCanvasLayers(canvas)
  const renderer = new MinimapRenderer(layers)
  const topRow = mode === 'proportional' ? 990n : 0n
  const geometry = createMinimapGeometry({
    mode, totalRows: 1000n, heightPx: 100, rowPx: 2, topRow, visibleMainRows: 10n,
  })
  const rows = new Map<bigint, MinimapRow>()
  for (const position of samplePlan(geometry)) {
    const row = position.row
    rows.set(row, {
      row, bytes: Array(bytesPerRow).fill(0x41),
      modifiedOffsets: [row * BigInt(bytesPerRow) + BigInt(bytesPerRow - 1)],
    })
  }
  renderer.resize(84, geometry.canvasHeightPx, 1)
  renderer.setContent(geometry, rows, { bytesPerRow, renderCharacters: false, scale: 1, theme: 'dark' })
  renderer.setViewport(viewportBox(geometry, topRow, topRow + 10n))
  renderer.paint()

  const bottom = pixel(layers.base, 4, 99)
  const finalColumnX = Math.floor(4 + (bytesPerRow - 1) * (84 - 8) / bytesPerRow)
  const finalColumn = pixel(layers.base, finalColumnX, 99)
  const borderY = mode === 'proportional' ? 80 : 0
  return {
    bottomRowPainted: differs(bottom.slice(0, 3), background),
    lastColumnPainted: finalColumn.slice(0, 3).every((value, index) => value === orange[index]),
    boxBorderPainted: differs(pixel(layers.visible, 0, borderY), pixel(layers.base, 0, borderY)),
    backingHeight: canvas.height,
  }
}

backend.readPage = async (offset, length) => ({
  offset: offset.toString(), bytes: Array(length).fill(0x41), modifiedOffsets: [], revision: '1',
})
backend.readMinimapSamples = async (rows, bytesPerRow, _sourceKey, revision) => ({
  revision, samples: rows.map((row) => ({
    row: row.toString(), bytes: Array(bytesPerRow).fill(0x41), modifiedOffsets: [],
  })),
})

async function renderComponent(bytesPerRow: BytesPerRow, mode: MinimapMode): Promise<ComponentResult> {
  const host = document.createElement('div')
  host.style.cssText = 'width:900px;height:400px;position:relative;'
  document.body.append(host)
  const page = ref<ViewportPage | null>(null)
  const app = createApp({
    render: () => h(HexCanvas, {
      fileSize: 1000n * BigInt(bytesPerRow), sourceIdentity: 1, sourceKey: 'visual.bin', sourceRevision: '1',
      page: page.value, bytesPerRow, selection: null, matches: [], templateRange: null,
      editMode: false, theme: 'dark', minimapSettings: { enabled: true, mode, renderCharacters: false, scale: 1 },
      'onRequest-page': (request: PageRequest) => {
        page.value = { offset: request.offset.toString(), bytes: Array(request.length).fill(0x41),
          modifiedOffsets: [], revision: '1', generation: request.generation }
      },
    }),
  })
  app.mount(host)
  try {
    for (let frame = 0; frame < 120; frame += 1) {
      if (page.value && host.querySelector('[data-page-revision="1"]')) break
      await new Promise<void>((resolveFrame) => requestAnimationFrame(() => resolveFrame()))
    }
    await nextTick()
    await new Promise<void>((resolveFrame) => requestAnimationFrame(() => requestAnimationFrame(() => resolveFrame())))
    const hex = host.querySelector<HTMLElement>('.hex-canvas')
    const mini = host.querySelector<HTMLElement>('.minimap-content')
    const preview = host.querySelector<HTMLCanvasElement>('[data-testid="minimap-preview"]')
    const gutter = host.querySelector<HTMLCanvasElement>('[data-testid="address-gutter"]')
    if (!hex || !mini || !preview || !gutter || !page.value) throw new Error('HexCanvas fixture did not mount and accept its page.')
    const pixels = gutter.getContext('2d')!.getImageData(0, 0, Math.min(gutter.width, 120), Math.min(gutter.height, 120)).data
    let gutterInk = false
    for (let index = 0; index < pixels.length; index += 4) {
      if (pixels[index] !== 0x11 || pixels[index + 1] !== 0x14 || pixels[index + 2] !== 0x18) {
        gutterInk = true
        break
      }
    }
    return {
      addressGutterPainted: gutterInk,
      hexEndsBeforeMinimap: hex.getBoundingClientRect().right <= mini.getBoundingClientRect().left + 1,
      minimapFillsHeight: Math.abs(preview.getBoundingClientRect().height - mini.getBoundingClientRect().height) <= 1,
      previewHeight: preview.getBoundingClientRect().height,
      minimapHeight: mini.getBoundingClientRect().height,
    }
  } finally {
    app.unmount()
    host.remove()
  }
}

async function run(): Promise<void> {
  await document.fonts.ready
  window.__hexforgeVisualResults = {
    fit16: render('fit', 16),
    fit32: render('fit', 32),
    proportional16: render('proportional', 16),
    proportional32: render('proportional', 32),
    component16Fit: await renderComponent(16, 'fit'),
    component32Proportional: await renderComponent(32, 'proportional'),
    templateDialogButtons: renderTemplateDialogButtons(),
    readability: (['fit', 'proportional'] as const).flatMap((mode) =>
      ([16, 32] as const).flatMap((bytesPerRow) => (['dark', 'light'] as const).flatMap((theme) =>
        [1, 1.25, 2].map((dpr) => renderReadability(mode, bytesPerRow, theme, dpr))))),
  }
}

void run().catch((error: unknown) => { window.__hexforgeVisualError = error instanceof Error ? error.message : String(error) })
