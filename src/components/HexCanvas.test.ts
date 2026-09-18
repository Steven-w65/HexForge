import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createLayout } from '../hex/layout'
import HexCanvas from './HexCanvas.vue'

let resizeCallback: ResizeObserverCallback
let drawnColors: string[]

class TestResizeObserver {
  constructor(callback: ResizeObserverCallback) { resizeCallback = callback }
  observe() {}
  disconnect() {}
  unobserve() {}
}

function context(): CanvasRenderingContext2D {
  return {
    canvas: null,
    fillStyle: '', strokeStyle: '', font: '', textBaseline: 'alphabetic', lineWidth: 1,
    globalAlpha: 1,
    clearRect: vi.fn(), fillRect: vi.fn(), strokeRect: vi.fn(),
    fillText(this: CanvasRenderingContext2D) { drawnColors.push(String(this.fillStyle)) },
    drawImage: vi.fn(),
    setTransform: vi.fn(), save: vi.fn(), restore: vi.fn(),
  } as unknown as CanvasRenderingContext2D
}

const readyProps = {
  fileSize: 4096n,
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
    vi.stubGlobal('ResizeObserver', TestResizeObserver)
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { callback(0); return 1 })
    vi.stubGlobal('cancelAnimationFrame', () => {})
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation(function (this: HTMLCanvasElement) {
      const value = context()
      Object.defineProperty(value, 'canvas', { value: this })
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

  it('requests a generation-tagged bounded page after resize', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
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

  it('emits edit requests only while edit mode is enabled', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    await wrapper.get('canvas').trigger('dblclick', point(4))
    expect(wrapper.emitted('edit-request')).toBeUndefined()
    await wrapper.setProps({ editMode: true })
    await wrapper.get('canvas').trigger('dblclick', point(4))
    expect(wrapper.emitted('edit-request')?.at(-1)).toEqual([4n])
  })

  it('uses wheel input to move the bigint virtual viewport', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    await wrapper.get('canvas').trigger('wheel', { deltaY: 100 })
    expect(wrapper.emitted('viewport-offset')?.at(-1)).toEqual([16n])
    const request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint }
    expect(request.offset).toBe(0n)
    expect(wrapper.get('.virtual-scrollbar__thumb').attributes('style')).toContain('translateY(')
    const movedStyle = wrapper.get('.virtual-scrollbar__thumb').attributes('style')
    expect(movedStyle).not.toContain('translateY(0px)')
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
    await resize(920, 500)
    await wrapper.setProps({ page: { offset: first.offset.toString(), bytes: [0x42], modifiedOffsets: [], revision: '2', generation: first.generation } })
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBeUndefined()
  })

  it('clears accepted bytes when a new request starts or the page becomes null', async () => {
    const wrapper = mount(HexCanvas, { props: readyProps })
    await resize()
    let request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; generation: number }
    await wrapper.setProps({ page: { offset: request.offset.toString(), bytes: [0x41], modifiedOffsets: [], revision: '1', generation: request.generation } })
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBe('1')
    await wrapper.get('canvas').trigger('wheel', { deltaY: 100 })
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBeUndefined()
    request = wrapper.emitted('request-page')?.at(-1)?.[0] as { offset: bigint; generation: number }
    await wrapper.setProps({ page: { offset: request.offset.toString(), bytes: [0x42], modifiedOffsets: [], revision: '2', generation: request.generation } })
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBe('2')
    await wrapper.setProps({ page: null })
    expect(wrapper.get('canvas').attributes('data-page-revision')).toBeUndefined()
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
