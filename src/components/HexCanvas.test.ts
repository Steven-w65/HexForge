import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createLayout } from '../hex/layout'
import { backend } from '../api/backend'
import type { MinimapSamplesResponse } from '../types'
import HexCanvas from './HexCanvas.vue'

let resizeCallback: ResizeObserverCallback
let drawnColors: string[]
let drawnTextByContext: string[][]
let contextIndex: number
let contextCanvases: Array<HTMLCanvasElement | undefined>
let contentCompositeCount: number
let fillCallsByContext: Array<Array<{ x: number; y: number; width: number; height: number; color: string }>>
let imageCopiesByContext: Array<Array<{ image: CanvasImageSource; arguments: number[] }>>

class TestResizeObserver {
  constructor(callback: ResizeObserverCallback) { resizeCallback = callback }
  observe() {}
  disconnect() {}
  unobserve() {}
}

function context(index: number): CanvasRenderingContext2D {
  return {
    canvas: null,
    fillStyle: '', strokeStyle: '', font: '', textBaseline: 'alphabetic', lineWidth: 1,
    globalAlpha: 1,
    clearRect: vi.fn(),
    fillRect(this: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
      ;(fillCallsByContext[index] ??= []).push({ x, y, width, height, color: String(this.fillStyle) })
    },
    strokeRect: vi.fn(),
    fillText(this: CanvasRenderingContext2D, text: string) {
      drawnColors.push(String(this.fillStyle))
      ;(drawnTextByContext[index] ??= []).push(text)
    },
    drawImage(image: CanvasImageSource, ...args: number[]) {
      ;(imageCopiesByContext[index] ??= []).push({ image, arguments: args })
      if (index === 0 && image === contextCanvases[2]) contentCompositeCount += 1
    },
    setTransform: vi.fn(), save: vi.fn(), restore: vi.fn(),
  } as unknown as CanvasRenderingContext2D
}

const readyProps = {
  fileSize: 4096n,
  sourceIdentity: 1,
  sourceKey: 'input.bin',
  sourceRevision: '1',
  page: null,
  bytesPerRow: 16 as const,
  selection: null,
  matches: [] as bigint[],
  templateRange: null,
  editMode: false,
  theme: 'dark' as const,
}

async function resize(width = 900, height = 500) {
  await nextTick()
  await nextTick()
  resizeCallback([{ contentRect: { width, height } } as ResizeObserverEntry], {} as ResizeObserver)
  await Promise.resolve()
}

function point(index: number, bytesPerRow: 16 | 32 = 16) {
  const layout = createLayout(900, bytesPerRow)
  return { clientX: layout.hexX + index * layout.byteStride + 2, clientY: layout.headerHeight + 5, pointerId: 1 }
}

describe('HexCanvas', () => {
  beforeEach(() => {
    resizeCallback = undefined as unknown as ResizeObserverCallback
    drawnColors = []
    drawnTextByContext = [[], [], [], []]
    contextIndex = 0
    contextCanvases = []
    contentCompositeCount = 0
    fillCallsByContext = []
    imageCopiesByContext = []
    vi.stubGlobal('ResizeObserver', TestResizeObserver)
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { callback(0); return 1 })
    vi.stubGlobal('cancelAnimationFrame', () => {})
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation(function (this: HTMLCanvasElement) {
      const value = context(contextIndex++)
      Object.defineProperty(value, 'canvas', { value: this })
      contextCanvases[contextIndex - 1] = this
      return value
    })
    HTMLCanvasElement.prototype.setPointerCapture = vi.fn()
    HTMLCanvasElement.prototype.releasePointerCapture = vi.fn()
  })

  it('emits an inclusive selection while dragging', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    const canvas = wrapper.get('canvas')
    await canvas.trigger('pointerdown', point(2))
    await canvas.trigger('pointermove', point(5))
    await canvas.trigger('pointerup', point(5))
    expect(wrapper.emitted('select')?.at(-1)).toEqual([{ start: 2n, end: 5n, count: 4n }])
  })

  it('fills the available stage before ResizeObserver measures it', () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    const viewport = wrapper.get<HTMLElement>('[data-testid="hex-canvas"]').element
    expect(viewport.style.width).toBe('100%')
    expect(viewport.style.height).toBe('100%')
  })

  it('keeps the minimap as a separate column and leaves the overview ruler when hidden', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    expect(wrapper.get('.hex-canvas').element.nextElementSibling?.classList.contains('minimap-content')).toBe(true)
    expect(wrapper.get('.minimap-content').element.nextElementSibling?.classList.contains('virtual-scrollbar')).toBe(true)
    const firstPreview = wrapper.get<HTMLCanvasElement>('[data-testid="minimap-preview"]').element
    await wrapper.setProps({ minimapSettings: { enabled: false, mode: 'fit', renderCharacters: true, scale: 1 } })
    expect(wrapper.find('[data-testid="minimap-preview"]').exists()).toBe(false)
    expect(wrapper.find('.virtual-scrollbar').exists()).toBe(true)
    expect(wrapper.get('.hex-canvas').attributes('style')).toContain('width: 100%')
    await wrapper.setProps({ minimapSettings: { enabled: true, mode: 'fit', renderCharacters: true, scale: 1 } })
    await nextTick()
    const restored = wrapper.get<HTMLCanvasElement>('[data-testid="minimap-preview"]').element
    expect(contextCanvases).toContain(restored)
    expect(restored).not.toBe(firstPreview)
  })

  it('keeps a short fit canvas at natural height and fills the viewport for a long file', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 32n } })
    await resize(900, 500)
    expect(wrapper.get<HTMLCanvasElement>('[data-testid="minimap-preview"]').element.style.height).toBe('4px')
    await wrapper.setProps({ fileSize: 1_000_000n })
    expect(wrapper.get<HTMLCanvasElement>('[data-testid="minimap-preview"]').element.style.height).toBe('500px')
    expect(wrapper.get('.minimap-content').element.getAttribute('style')).toContain('height: 100%')
  })

  it('keeps an empty file minimap at zero visible height', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 0n } })
    await resize(900, 500)
    expect(wrapper.get<HTMLCanvasElement>('[data-testid="minimap-preview"]').element.style.height).toBe('0px')
  })

  it('uses the shaded box instead of an internal proportional scrollbar', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 1_000_000n,
      minimapSettings: { enabled: true, mode: 'proportional' as const, renderCharacters: true, scale: 1 as const },
    } })
    await resize()
    expect(wrapper.find('.minimap-internal-scrollbar').exists()).toBe(false)
    expect(wrapper.find('.virtual-scrollbar').exists()).toBe(true)
    await wrapper.setProps({ minimapSettings: { enabled: true, mode: 'fit', renderCharacters: true, scale: 1 } })
    expect(wrapper.find('.minimap-internal-scrollbar').exists()).toBe(false)
  })

  it('drags the proportional box to the last full page while keeping its pointer offset', async () => {
    const readPage = vi.spyOn(backend, 'readPage').mockImplementation(async (offset, length) => ({
      offset: offset.toString(), bytes: Array(length).fill(0x41), modifiedOffsets: [], revision: '1',
    }))
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 16_000n,
      minimapSettings: { enabled: true, mode: 'proportional' as const, renderCharacters: true, scale: 1 as const },
    } })
    try {
      await resize(900, 252) // header 32 + exactly ten fully visible 22px rows
      const request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; length: number; generation: number }
      await wrapper.setProps({ page: { offset: request.offset.toString(), bytes: Array(request.length).fill(0x41),
        modifiedOffsets: [], revision: '1', generation: request.generation } })
      const container = wrapper.get('.minimap-content')
      vi.spyOn(container.element, 'getBoundingClientRect').mockReturnValue({
        top: 0, height: 252, left: 0, right: 84, bottom: 252, width: 84, x: 0, y: 0, toJSON: () => ({}),
      })
      const preview = wrapper.get<HTMLCanvasElement>('[data-testid="minimap-preview"]')
      await preview.trigger('pointerdown', { clientX: 20, clientY: 7, pointerId: 81 })
      await preview.trigger('pointermove', { clientX: 20, clientY: 239, pointerId: 81, buttons: 1 })
      expect(container.classes()).toContain('is-box-dragging')
      expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([990n * 16n])
      expect(preview.element.height).toBeLessThanOrEqual(252 * (globalThis.devicePixelRatio || 1))
      const previewIndex = contextCanvases.indexOf(preview.element)
      expect(fillCallsByContext[previewIndex]?.at(-1)).toMatchObject({ y: 232, height: 20 })
      await vi.waitFor(() => expect(readPage.mock.calls.some(([offset]) => offset === 874n * 16n)).toBe(true))
      await preview.trigger('pointerup', { clientX: 20, clientY: 300, pointerId: 81 })
      expect(container.classes()).not.toContain('is-box-dragging')
      expect(container.classes()).not.toContain('is-box-hover')
    } finally { wrapper.unmount(); readPage.mockRestore() }
  })

  it('renders all 32 bytes of a row when the hex viewer uses 32 bytes per row', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 32n, bytesPerRow: 32 } })
    await resize()
    const request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; length: number; generation: number }
    const previewIndex = contextCanvases.indexOf(wrapper.get<HTMLCanvasElement>('[data-testid="minimap-preview"]').element)
    drawnTextByContext[previewIndex + 1] = []
    await wrapper.setProps({ page: { offset: request.offset.toString(), bytes: Array(request.length).fill(0x41), modifiedOffsets: [], revision: '1', generation: request.generation } })
    expect(drawnTextByContext[previewIndex + 1]).toHaveLength(32)
  })

  it('keeps minimap marker content on Canvas and overview markers in the ruler', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, matches: [0n] } })
    await resize()
    const previewIndex = contextCanvases.indexOf(wrapper.get<HTMLCanvasElement>('[data-testid="minimap-preview"]').element)
    expect(wrapper.find('.minimap-content .minimap__match').exists()).toBe(false)
    expect(wrapper.find('.virtual-scrollbar .minimap__match').exists()).toBe(true)
    expect(fillCallsByContext[previewIndex + 2]?.some((call) => call.color === '#bda64a')).toBe(true)
  })

  it('requests a generation-tagged bounded page after resize', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await nextTick()
    expect(wrapper.emitted('request-page')).toBeUndefined()
    await resize()
    expect(wrapper.emitted('request-page')).toHaveLength(1)
    const request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; length: number; generation: number }
    expect(request.length).toBeGreaterThan(0)
    expect(request.length).toBeLessThanOrEqual(1024 * 1024)
    expect(request.generation).toBeGreaterThan(0)
  })

  it('projects file-wide markers into the shared overview ruler with overlap priority', async () => {
    const wrapper = mount(HexCanvas, { props: {
      ...readyProps, fileSize: 100n, matches: [0n, 50n, 99n],
      templateFields: [{ name: 'header', offset: '20', type: 'bytes' as const, length: 10, endianness: 'little' as const, value: '', comment: '' }],
      modifiedOverview: { binCount: 1024, bins: [0, 512, 1023] },
    } })
    await resize()

    const markerCount = wrapper.findAll('.minimap__match, .minimap__template, .minimap__modified').length
    expect(markerCount).toBeGreaterThan(0)
    expect(markerCount).toBeLessThanOrEqual(500)
    expect(wrapper.findAll('.minimap__modified').length).toBeGreaterThan(0)
    expect(wrapper.emitted('request-page')).toHaveLength(1)
    expect(wrapper.findAll('.minimap__modified').at(-1)?.attributes('style')).toContain('top: 499px')
  })

  it('draws edited bytes into the minimap base layer from an accepted main page', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 1_000_000n } })
    await resize(900, 500)
    const request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; length: number; generation: number }
    expect(request.length).toBeLessThan(16_384)
    const preview = wrapper.get<HTMLCanvasElement>('[data-testid="minimap-preview"]').element
    await wrapper.setProps({ page: {
      offset: request.offset.toString(), bytes: Array.from({ length: request.length }, (_, index) => [0, 0x41, 0x7f, 0xff][index % 4]!),
      modifiedOffsets: ['1'], revision: '1', generation: request.generation,
    } })
    expect(contextCanvases).toContain(preview)
    expect(fillCallsByContext.some((calls) => calls.some((call) => call.color === '#f0883e'))).toBe(true)
    expect(wrapper.emitted('request-page')).toHaveLength(1)
  })

  it('reports a sparse sample failure nonfatally without dropping accepted hex bytes', async () => {
    const sparse = vi.spyOn(backend, 'readMinimapSamples').mockRejectedValue({ code: 'source_changed', message: 'Source file changed.' })
    const wrapper = mount(HexCanvas, { props: { ...readyProps, sourceKey: 'error-case.bin', fileSize: 1_000_000n } })
    try {
      await resize()
      const request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; length: number; generation: number }
      await wrapper.setProps({ page: { offset: request.offset.toString(), bytes: Array(request.length).fill(0x41), modifiedOffsets: [], revision: '1', generation: request.generation } })
      await new Promise((resolve) => setTimeout(resolve, 20))
      expect(wrapper.emitted('minimap-error')).toHaveLength(1)
      expect(wrapper.get('canvas').attributes('data-page-revision')).toBe('1')
      expect(sparse.mock.calls.some((call) => call[2] === 'error-case.bin')).toBe(true)
    } finally { wrapper.unmount(); sparse.mockRestore() }
  })

  it('shows an inline retry for a minimap-only failure without opening the app error dialog', async () => {
    const sparse = vi.spyOn(backend, 'readMinimapSamples')
      .mockRejectedValueOnce({ code: 'operation_failed', message: 'The preview could not be read.' })
      .mockImplementation(async (rows) => ({ revision: '1', samples: rows.map((row) => ({
        row: row.toString(), bytes: Array(16).fill(0x41), modifiedOffsets: [],
      })) }))
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 1_000_000n } })
    try {
      await resize()
      const request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; length: number; generation: number }
      await wrapper.setProps({ page: { offset: request.offset.toString(), bytes: Array(request.length).fill(0x41),
        modifiedOffsets: [], revision: '1', generation: request.generation } })
      await vi.waitFor(() => expect(wrapper.find('[data-testid="minimap-retry"]').exists()).toBe(true))
      expect(wrapper.emitted('minimap-error')).toBeUndefined()
      expect(wrapper.get('canvas').attributes('data-page-revision')).toBe('1')

      const callsBeforeRetry = sparse.mock.calls.length
      await wrapper.get('[data-testid="minimap-retry"]').trigger('click')
      await vi.waitFor(() => expect(sparse.mock.calls.length).toBeGreaterThan(callsBeforeRetry))
      await vi.waitFor(() => expect(wrapper.find('[data-testid="minimap-retry"]').exists()).toBe(false))
    } finally { wrapper.unmount(); sparse.mockRestore() }
  })

  it('ignores an old sparse batch after switching to another file with the same revision', async () => {
    let resolveOld!: (value: MinimapSamplesResponse) => void
    const sparse = vi.spyOn(backend, 'readMinimapSamples').mockImplementation((_rows, _width, sourceKey) => sourceKey === 'old.bin'
      ? new Promise<MinimapSamplesResponse>((resolve) => { resolveOld = resolve })
      : Promise.resolve({ revision: '1', samples: [] }))
    const wrapper = mount(HexCanvas, { props: { ...readyProps, sourceKey: 'old.bin', fileSize: 1_000_000n } })
    try {
      await resize()
      const request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; length: number; generation: number }
      await wrapper.setProps({ page: { offset: request.offset.toString(), bytes: Array(request.length).fill(0x41), modifiedOffsets: [], revision: '1', generation: request.generation } })
      await new Promise((resolve) => setTimeout(resolve, 20))
      const oldRows = sparse.mock.calls.find((call) => call[2] === 'old.bin')?.[0]
      expect(oldRows?.length).toBeGreaterThan(0)
      await wrapper.setProps({ sourceIdentity: 2, sourceKey: 'new.bin', page: null })
      const orangeBefore = fillCallsByContext.flat().filter((call) => call.color === '#f0883e').length
      resolveOld({ revision: '1', samples: oldRows!.map((row) => ({ row: row.toString(), bytes: Array(16).fill(0x41), modifiedOffsets: [(row * 16n).toString()] })) })
      await new Promise((resolve) => setTimeout(resolve, 20))
      expect(fillCallsByContext.flat().filter((call) => call.color === '#f0883e')).toHaveLength(orangeBefore)
      expect(wrapper.get('canvas').attributes('data-page-revision')).toBeUndefined()
    } finally { wrapper.unmount(); sparse.mockRestore() }
  })

  it('uses the file-wide minimap for navigation and retains the separate full-file scroll strip', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 1_000_000n } })
    await resize(900, 500)
    const request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; length: number; generation: number }
    await wrapper.setProps({ page: { offset: request.offset.toString(), bytes: Array(request.length).fill(0x41), modifiedOffsets: [], revision: '1', generation: request.generation } })
    const preview = wrapper.get('[data-testid="minimap-preview"]')
    vi.spyOn(preview.element, 'getBoundingClientRect').mockReturnValue({
      top: 0, height: 500, left: 0, right: 84, bottom: 500, width: 84, x: 0, y: 0, toJSON: () => ({}),
    })
    await preview.trigger('pointerdown', { clientY: 180, pointerId: 13 })
    const selectedOffset = wrapper.emitted('viewport-offset')?.at(-1)?.[0] as bigint
    expect(selectedOffset).toBeGreaterThan(100_000n)
    expect(selectedOffset).toBeLessThan(500_000n)

    const scrollbar = wrapper.get('.virtual-scrollbar')
    vi.spyOn(scrollbar.element, 'getBoundingClientRect').mockReturnValue({
      top: 0, height: 500, left: 84, right: 96, bottom: 500, width: 12, x: 84, y: 0, toJSON: () => ({}),
    })
    await scrollbar.trigger('pointerdown', { clientY: 499, pointerId: 14 })
    expect(wrapper.emitted('viewport-offset')?.at(-1)?.[0] as bigint).toBeGreaterThan(500_000n)
  })

  it('links minimap dragging and wheel movement to the main viewport', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 1_000_000n } })
    await resize(900, 500)
    const preview = wrapper.get('[data-testid="minimap-preview"]')
    vi.spyOn(preview.element, 'getBoundingClientRect').mockReturnValue({
      top: 0, height: 500, left: 0, right: 84, bottom: 500, width: 84, x: 0, y: 0, toJSON: () => ({}),
    })
    await preview.trigger('pointerdown', { clientY: 100, pointerId: 17 })
    const clicked = wrapper.emitted('viewport-offset')?.at(-1)?.[0] as bigint
    await preview.trigger('pointermove', { clientY: 150, pointerId: 17, buttons: 1 })
    const dragged = wrapper.emitted('viewport-offset')?.at(-1)?.[0] as bigint
    expect(dragged).toBeGreaterThan(clicked)
    await preview.trigger('pointerup', { clientY: 150, pointerId: 17 })
    await preview.trigger('pointermove', { clientY: 250, pointerId: 17, buttons: 1 })
    expect(wrapper.emitted('viewport-offset')?.at(-1)?.[0]).toBe(dragged)
    await wrapper.get('.minimap-content').trigger('wheel', { deltaY: 40 })
    expect(wrapper.emitted('viewport-offset')?.at(-1)?.[0] as bigint).toBeGreaterThan(dragged)
  })

  it('uses container coordinates for proportional dragging even when the canvas rect differs', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 16_000n,
      minimapSettings: { enabled: true, mode: 'proportional' as const, renderCharacters: true, scale: 1 as const },
    } })
    await resize(900, 100)
    await wrapper.setProps({ navigateOffset: 35n * 16n })
    const container = wrapper.get('.minimap-content')
    const preview = wrapper.get('[data-testid="minimap-preview"]')
    vi.spyOn(container.element, 'getBoundingClientRect').mockReturnValue({
      top: 0, height: 100, left: 0, right: 84, bottom: 100, width: 84, x: 0, y: 0, toJSON: () => ({}),
    })
    vi.spyOn(preview.element, 'getBoundingClientRect').mockReturnValue({
      top: 20, height: 100, left: 0, right: 84, bottom: 120, width: 84, x: 0, y: 20, toJSON: () => ({}),
    })
    await preview.trigger('pointerdown', { clientY: 4, pointerId: 51 })
    await preview.trigger('pointermove', { clientY: 14, pointerId: 51, buttons: 1 })
    expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([141n * 16n])
  })

  it('keeps a captured proportional drag active when the pointer leaves the minimap', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 16_000n,
      minimapSettings: { enabled: true, mode: 'proportional' as const, renderCharacters: true, scale: 1 as const },
    } })
    await resize(900, 100)
    await wrapper.setProps({ navigateOffset: 35n * 16n })
    const container = wrapper.get('.minimap-content')
    const preview = wrapper.get('[data-testid="minimap-preview"]')
    await preview.trigger('pointerdown', { clientY: 4, pointerId: 52 })
    await container.trigger('pointerleave', { clientY: 4, pointerId: 52 })
    await preview.trigger('pointermove', { clientY: 14, pointerId: 52, buttons: 1 })
    expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([141n * 16n])
    await preview.trigger('pointerup', { clientY: 14, pointerId: 52 })
    await preview.trigger('pointermove', { clientY: 20, pointerId: 52, buttons: 1 })
    expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([141n * 16n])
  })

  it('clamps a captured drag outside the container then follows it back in', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 16_000n,
      minimapSettings: { enabled: true, mode: 'proportional' as const, renderCharacters: true, scale: 1 as const },
    } })
    await resize(900, 100)
    await wrapper.setProps({ navigateOffset: 35n * 16n })
    const container = wrapper.get('.minimap-content')
    vi.spyOn(container.element, 'getBoundingClientRect').mockReturnValue({
      top: 0, height: 100, left: 0, right: 84, bottom: 100, width: 84, x: 0, y: 0, toJSON: () => ({}),
    })
    const preview = wrapper.get('[data-testid="minimap-preview"]')
    await preview.trigger('pointerdown', { clientX: 20, clientY: 4, pointerId: 55 })
    await preview.trigger('pointermove', { clientX: 20, clientY: 110, pointerId: 55, buttons: 1 })
    expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([997n * 16n])
    await preview.trigger('pointermove', { clientX: 20, clientY: 14, pointerId: 55, buttons: 1 })
    expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([141n * 16n])
  })

  it('does not release an already lost minimap pointer capture twice', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 16_000n,
      minimapSettings: { enabled: true, mode: 'proportional' as const, renderCharacters: true, scale: 1 as const },
    } })
    await resize(900, 100)
    const preview = wrapper.get<HTMLCanvasElement>('[data-testid="minimap-preview"]')
    Object.defineProperty(preview.element, 'hasPointerCapture', { value: () => false, configurable: true })
    vi.spyOn(preview.element, 'releasePointerCapture').mockImplementation(() => { throw new Error('Pointer capture was already lost') })
    await preview.trigger('pointerdown', { clientY: 4, pointerId: 54 })
    await preview.trigger('lostpointercapture', { clientY: 4, pointerId: 54 })
    await preview.trigger('pointermove', { clientY: 14, pointerId: 54, buttons: 1 })
    expect(wrapper.emitted('viewport-offset')).toBeUndefined()
  })

  it('coalesces rapid minimap wheel input into one navigation frame', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 16_000n,
      minimapSettings: { enabled: true, mode: 'proportional' as const, renderCharacters: true, scale: 1 as const },
    } })
    await resize(900, 100)
    const pending: FrameRequestCallback[] = []
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { pending.push(callback); return pending.length })
    const before = wrapper.emitted('viewport-offset')?.length ?? 0
    await wrapper.get('.minimap-content').trigger('wheel', { deltaY: 44 })
    await wrapper.get('.minimap-content').trigger('wheel', { deltaY: 44 })
    await wrapper.get('.minimap-content').trigger('wheel', { deltaY: 44 })
    expect(wrapper.emitted('viewport-offset')?.length ?? 0).toBe(before)
    expect(pending).toHaveLength(1)
    pending.shift()!(0)
    expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([3n * 16n])
    wrapper.unmount()
  })

  it('discards queued minimap navigation when the source file changes', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 16_000n,
      minimapSettings: { enabled: true, mode: 'proportional' as const, renderCharacters: true, scale: 1 as const },
    } })
    await resize(900, 100)
    const pending: FrameRequestCallback[] = []
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { pending.push(callback); return pending.length })
    await wrapper.get('.minimap-content').trigger('wheel', { deltaY: 44 })
    await wrapper.setProps({ sourceIdentity: 2, sourceKey: 'replacement.bin', page: null })
    pending.shift()!(0)
    expect(wrapper.emitted('viewport-offset')?.at(-1)?.[0]).not.toBe(32n)
    wrapper.unmount()
  })

  it('prefetches newly exposed proportional tiles while dragging toward EOF', async () => {
    const readPage = vi.spyOn(backend, 'readPage').mockImplementation(async (offset, length) => ({
      offset: offset.toString(), bytes: Array(length).fill(0x41), modifiedOffsets: [], revision: '1',
    }))
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 16_000n,
      minimapSettings: { enabled: true, mode: 'proportional' as const, renderCharacters: true, scale: 1 as const },
    } })
    try {
      await resize(900, 100)
      const request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; length: number; generation: number }
      await wrapper.setProps({ page: {
        offset: request.offset.toString(), bytes: Array(request.length).fill(0x41), modifiedOffsets: [],
        revision: '1', generation: request.generation,
      } })
      await wrapper.setProps({ navigateOffset: 900n * 16n })
      await vi.waitFor(() => expect(readPage.mock.calls.some(([offset]) => offset >= 900n * 16n)).toBe(true))
      readPage.mockClear()
      const preview = wrapper.get('[data-testid="minimap-preview"]')
      await preview.trigger('pointerdown', { clientY: 87, pointerId: 53 })
      await preview.trigger('pointermove', { clientY: 110, pointerId: 53, buttons: 1 })
      await vi.waitFor(() => expect(readPage.mock.calls.some(([offset]) => offset >= 940n * 16n)).toBe(true))
    } finally {
      wrapper.unmount()
      readPage.mockRestore()
    }
  })

  it('uses a proportional preview click for navigation only', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 16_000n,
      minimapSettings: { enabled: true, mode: 'proportional' as const, renderCharacters: true, scale: 1 as const },
    } })
    await resize(900, 100)
    const preview = wrapper.get('[data-testid="minimap-preview"]')
    await preview.trigger('pointerdown', { clientY: 60, pointerId: 71 })
    expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([30n * 16n])
    await preview.trigger('pointermove', { clientY: 80, pointerId: 71, buttons: 1 })
    expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([30n * 16n])
  })

  it('bounds dense search markers to at most one per minimap pixel', async () => {
    const wrapper = mount(HexCanvas, { props: {
      ...readyProps, fileSize: 10_000n, matches: Array.from({ length: 10_000 }, (_, index) => BigInt(index)),
    } })
    await resize(900, 500)
    const markers = wrapper.findAll('.minimap__match')
    expect(markers.length).toBeGreaterThan(0)
    expect(markers.length).toBeLessThanOrEqual(500)
  })

  it('uses a proportional viewport marker and stops at the last full visible page', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize(900, 500)
    const thumb = wrapper.get('.virtual-scrollbar__thumb')
    expect(Number.parseFloat(thumb.element.getAttribute('style')?.match(/height:\s*([\d.]+)px/)?.[1] ?? '0')).toBeGreaterThan(24)
    const minimap = wrapper.get('.virtual-scrollbar')
    vi.spyOn(minimap.element, 'getBoundingClientRect').mockReturnValue({
      top: 0, height: 500, left: 0, right: 40, bottom: 500, width: 40, x: 0, y: 0, toJSON: () => ({}),
    })
    await minimap.trigger('pointerdown', { clientY: 499, pointerId: 7 })
    expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([3760n])
  })

  it('lets the last row become fully visible when the viewport has a partial row', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: 64n } })
    await resize(900, 100)
    const minimap = wrapper.get('.virtual-scrollbar')
    vi.spyOn(minimap.element, 'getBoundingClientRect').mockReturnValue({
      top: 0, height: 100, left: 0, right: 40, bottom: 100, width: 40, x: 0, y: 0, toJSON: () => ({}),
    })
    await minimap.trigger('pointerdown', { clientY: 99, pointerId: 10 })
    expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([16n])
  })

  it('navigates from a minimap search marker in a multi-gigabyte file', async () => {
    const size = 1n << 60n
    const wrapper = mount(HexCanvas, { props: { ...readyProps, fileSize: size, matches: [size * 3n / 4n] } })
    await resize(900, 500)
    const minimap = wrapper.get('.virtual-scrollbar')
    vi.spyOn(minimap.element, 'getBoundingClientRect').mockReturnValue({
      top: 0, height: 500, left: 0, right: 40, bottom: 500, width: 40, x: 0, y: 0, toJSON: () => ({}),
    })
    const markerY = Number.parseFloat(wrapper.get('.minimap__match').attributes('style')?.match(/top:\s*([\d.]+)px/)?.[1] ?? '0')
    await minimap.trigger('pointerdown', { clientY: markerY, pointerId: 8 })

    const offset = wrapper.emitted('viewport-offset')?.at(-1)?.[0] as bigint
    expect(offset).toBeGreaterThan(size / 2n)
    expect(offset).toBeLessThan(size)
    expect((wrapper.emitted('request-page')?.at(-1)?.[0] as { length: number }).length).toBeLessThanOrEqual(1024 * 1024)
  })

  it('recalculates hit geometry and requests a new page when row width changes', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    const before = wrapper.emitted('request-page')?.length ?? 0
    await wrapper.setProps({ bytesPerRow: 32 })
    expect(wrapper.emitted('request-page')?.length).toBeGreaterThan(before)
    const request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint }
    expect(request.offset % 32n).toBe(0n)
  })

  it('expands the horizontally scrollable Canvas extent for 32-byte rows', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize(500, 400)
    const sixteenWidth = Number.parseFloat((wrapper.get('[data-testid="hex-canvas"]').attributes('style') ?? '').match(/--canvas-width:\s*([\d.]+)px/)?.[1] ?? '0')
    await wrapper.setProps({ bytesPerRow: 32 })
    const thirtyTwoWidth = Number.parseFloat((wrapper.get('[data-testid="hex-canvas"]').attributes('style') ?? '').match(/--canvas-width:\s*([\d.]+)px/)?.[1] ?? '0')
    expect(sixteenWidth).toBeGreaterThan(500)
    expect(thirtyTwoWidth).toBeGreaterThan(sixteenWidth)
  })

  it('emits edit requests only while edit mode is enabled', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    await wrapper.get('canvas').trigger('dblclick', point(4))
    expect(wrapper.emitted('edit-request')).toBeUndefined()
    await wrapper.setProps({ editMode: true })
    await wrapper.get('canvas').trigger('dblclick', point(4))
    expect(wrapper.emitted('edit-request')?.at(-1)).toEqual([4n])
  })

  it('reuses a prefetched page for a one-row wheel movement without blanking or rereading', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    const initialRequest = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; length: number; generation: number }
    await wrapper.setProps({ page: {
      offset: initialRequest.offset.toString(),
      bytes: Array.from({ length: initialRequest.length }, () => 0x41),
      modifiedOffsets: [], revision: '1', generation: initialRequest.generation,
    } })
    const requestsBeforeMove = wrapper.emitted('request-page')?.length ?? 0
    drawnTextByContext[2]!.length = 0
    contentCompositeCount = 0
    await wrapper.get('canvas').trigger('wheel', { deltaY: 100 })
    expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([16n])
    expect(wrapper.emitted('request-page')).toHaveLength(requestsBeforeMove)
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBe('1')
    expect(drawnTextByContext[2]).toContain('41')
    expect(contentCompositeCount).toBeGreaterThan(0)
    expect(wrapper.get('.virtual-scrollbar__thumb').attributes('style')).toContain('translateY(')
    const movedStyle = wrapper.get('.virtual-scrollbar__thumb').attributes('style')
    expect(movedStyle).not.toContain('translateY(0px)')
  })

  it('navigates the virtual viewport when orchestration targets an offset', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    await wrapper.setProps({ navigateOffset: 160n })
    expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([160n])
    const request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint }
    expect(request.offset).toBeLessThanOrEqual(160n)
    expect(request.offset + 1024n).toBeGreaterThan(160n)
  })

  it('preserves the visible byte offset when changing between 16 and 32 columns', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    await wrapper.get('canvas').trigger('wheel', { deltaY: 100 })
    await wrapper.get('canvas').trigger('wheel', { deltaY: 100 })
    expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([32n])
    await wrapper.setProps({ bytesPerRow: 32 })
    await wrapper.get('canvas').trigger('pointerdown', point(0, 32))
    expect(wrapper.emitted('select')?.at(-1)).toEqual([{ start: 32n, end: 32n, count: 1n }])
    await wrapper.setProps({ bytesPerRow: 16 })
    await wrapper.get('canvas').trigger('pointerdown', point(0, 16))
    expect(wrapper.emitted('select')?.at(-1)).toEqual([{ start: 32n, end: 32n, count: 1n }])
  })

  it('does not draw a page tagged for an earlier request generation', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    const first = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; generation: number }
    await wrapper.setProps({ page: { offset: first.offset.toString(), bytes: [0x41], modifiedOffsets: [], revision: '1', generation: first.generation } })
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBe('1')
    await wrapper.setProps({ sourceRevision: '2', page: null })
    await wrapper.setProps({ page: { offset: first.offset.toString(), bytes: [0x42], modifiedOffsets: [], revision: '2', generation: first.generation } })
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBeUndefined()
  })

  it('keeps overlapping accepted bytes visible while a replacement page is pending', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    const request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; length: number; generation: number }
    await wrapper.setProps({ page: {
      offset: request.offset.toString(), bytes: Array.from({ length: request.length }, () => 0x41),
      modifiedOffsets: [], revision: '1', generation: request.generation,
    } })
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBe('1')
    const requestsBeforeMove = wrapper.emitted('request-page')?.length ?? 0
    drawnTextByContext[2]!.length = 0
    contentCompositeCount = 0
    await wrapper.setProps({ navigateOffset: 202n * 16n })
    expect(wrapper.emitted('request-page')?.length).toBeGreaterThan(requestsBeforeMove)
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBe('1')
    expect(drawnTextByContext[2]).toContain('41')
    expect(contentCompositeCount).toBeGreaterThan(0)
    await wrapper.setProps({ page: null })
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBeUndefined()
  })

  it('drops cached bytes and rejects responses from another file or revision', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    const first = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; generation: number }
    await wrapper.setProps({ page: { offset: first.offset.toString(), bytes: [0x41], modifiedOffsets: [], revision: '1', generation: first.generation } })
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBe('1')
    await wrapper.setProps({ sourceKey: 'replacement.bin', sourceRevision: '7', page: null })
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBeUndefined()
    const replacement = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; generation: number }
    await wrapper.setProps({ page: { offset: replacement.offset.toString(), bytes: [0x42], modifiedOffsets: [], revision: '1', generation: replacement.generation } })
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBeUndefined()
    await wrapper.setProps({ page: { offset: replacement.offset.toString(), bytes: [0x43], modifiedOffsets: [], revision: '7', generation: replacement.generation } })
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBe('7')
  })

  it('requests a measured page when the same pristine path and revision is reopened', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    const first = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; length: number; generation: number }
    await wrapper.setProps({ page: {
      offset: first.offset.toString(), bytes: Array.from({ length: first.length }, () => 0x41),
      modifiedOffsets: [], revision: '1', generation: first.generation,
    } })
    const beforeReopen = wrapper.emitted('request-page')?.length ?? 0
    await wrapper.setProps({ sourceIdentity: 2, page: null })
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBeUndefined()
    expect(wrapper.emitted('request-page')).toHaveLength(beforeReopen + 1)
    const reopened = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; length: number; generation: number }
    drawnTextByContext[2]!.length = 0
    await wrapper.setProps({ page: {
      offset: reopened.offset.toString(), bytes: Array.from({ length: reopened.length }, () => 0x42),
      modifiedOffsets: [], revision: '1', generation: reopened.generation,
    } })
    expect(drawnTextByContext[2]).toContain('42')
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBe('1')
  })

  it('resets horizontal scrolling when a different file is opened', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, bytesPerRow: 32 } })
    await resize(500, 400)
    const viewport = wrapper.get<HTMLElement>('[data-testid="hex-canvas"]').element
    viewport.scrollLeft = 120

    await wrapper.setProps({ sourceIdentity: 2, sourceKey: 'other.bin', page: null })

    expect(viewport.scrollLeft).toBe(0)
  })

  it('keeps a copied address gutter outside the horizontally scrolling canvas', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, bytesPerRow: 32 } })
    await resize(500, 400)
    const scroller = wrapper.get<HTMLElement>('[data-testid="hex-canvas"]').element
    const mainCanvas = scroller.querySelector('canvas')!
    const gutter = wrapper.get<HTMLCanvasElement>('[data-testid="address-gutter"]').element
    const gutterContextIndex = contextCanvases.indexOf(gutter)

    expect(gutter.parentElement).not.toBe(scroller)
    expect(drawnTextByContext[1]).toContain('OFFSET')
    expect(imageCopiesByContext[gutterContextIndex]?.some((copy) =>
      copy.image === mainCanvas && copy.arguments[0] === 0 && copy.arguments[1] === 0 &&
      copy.arguments[2] === gutter.width && copy.arguments[3] === gutter.height,
    )).toBe(true)
  })

  it('still scrolls rows when the pointer is over the fixed address gutter', async () => {
    const wrapper = mount(HexCanvas, { props: { ...readyProps, bytesPerRow: 32 } })
    await resize(500, 400)

    await wrapper.get('[data-testid="address-gutter"]').trigger('wheel', { deltaY: 100 })

    expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([32n])
  })

  it('preserves the viewport position when an edit advances the source revision', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    await wrapper.get('canvas').trigger('wheel', { deltaY: 100 })
    await wrapper.setProps({ sourceRevision: '2', page: null })
    await wrapper.get('canvas').trigger('pointerdown', point(0))
    expect(wrapper.emitted('select')?.at(-1)).toEqual([{ start: 16n, end: 16n, count: 1n }])
  })

  it('uses the retained viewport offset when Save As switches the active source', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    await wrapper.setProps({ navigateOffset: 160n })
    await wrapper.setProps({ sourceIdentity: 2, sourceKey: 'copy.bin', sourceRevision: '0', page: null })

    await wrapper.get('canvas').trigger('pointerdown', point(0))
    expect(wrapper.emitted('select')?.at(-1)).toEqual([{ start: 160n, end: 160n, count: 1n }])
    const request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; length: number }
    expect(request.offset).toBeLessThanOrEqual(160n)
    expect(request.offset + BigInt(request.length)).toBeGreaterThan(160n)
  })

  it('keeps the pointer grab offset while dragging across the effective scrollbar track', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize(900, 500)
    const scrollbar = wrapper.get('.virtual-scrollbar')
    vi.spyOn(scrollbar.element, 'getBoundingClientRect').mockReturnValue({
      top: 0, height: 500, left: 0, right: 8, bottom: 500, width: 8, x: 0, y: 0, toJSON: () => ({}),
    })
    await scrollbar.trigger('pointerdown', { clientY: 250, pointerId: 5 })
    const middleOffset = wrapper.emitted('viewport-offset')?.at(-1)?.[0] as bigint
    const thumbTop = Number.parseFloat((wrapper.get('.virtual-scrollbar__thumb').attributes('style') ?? '').match(/translateY\(([-\d.]+)px\)/)?.[1] ?? '0')
    await scrollbar.trigger('pointerdown', { clientY: thumbTop + 5, pointerId: 6 })
    await scrollbar.trigger('pointermove', { clientY: thumbTop + 105, pointerId: 6, buttons: 1 })
    const draggedOffset = wrapper.emitted('viewport-offset')?.at(-1)?.[0] as bigint
    expect(middleOffset).toBeGreaterThan(0n)
    expect(draggedOffset).toBeGreaterThan(middleOffset)
    const draggedTop = Number.parseFloat((wrapper.get('.virtual-scrollbar__thumb').attributes('style') ?? '').match(/translateY\(([-\d.]+)px\)/)?.[1] ?? '0')
    expect(Math.abs(draggedTop - (thumbTop + 100))).toBeLessThanOrEqual(2)
  })

  it('applies theme changes to the renderer and redraws cached text layers', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    drawnColors = []
    await wrapper.setProps({ theme: 'light' })
    expect(wrapper.get('[data-testid="hex-canvas"]').attributes('data-theme')).toBe('light')
    expect(drawnColors).toContain('#57606a')
  })
})
