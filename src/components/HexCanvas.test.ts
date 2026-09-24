import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createLayout } from '../hex/layout'
import HexCanvas from './HexCanvas.vue'

let resizeCallback: ResizeObserverCallback
let drawnColors: string[]
let drawnTextByContext: string[][]
let contextIndex: number
let contextCanvases: Array<HTMLCanvasElement | undefined>
let contentCompositeCount: number

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
    clearRect: vi.fn(), fillRect: vi.fn(), strokeRect: vi.fn(),
    fillText(this: CanvasRenderingContext2D, text: string) {
      drawnColors.push(String(this.fillStyle))
      drawnTextByContext[index]!.push(text)
    },
    drawImage(image: CanvasImageSource) {
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
    for (let index = 0; index < 5; index += 1) await wrapper.get('canvas').trigger('wheel', { deltaY: 100 })
    drawnTextByContext[2]!.length = 0
    contentCompositeCount = 0
    await wrapper.get('canvas').trigger('wheel', { deltaY: 100 })
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
