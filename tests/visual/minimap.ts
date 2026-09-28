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
  }
}

void run().catch((error: unknown) => { window.__hexforgeVisualError = error instanceof Error ? error.message : String(error) })
