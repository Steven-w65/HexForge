<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import type { ColorTheme, PageRequest, ViewportPage } from '../types'
import { contentWidth, createLayout, hitTestByte, visibleRange, type BytesPerRow, type HexLayout } from '../hex/layout'
import { normalizeSelection, type ByteSelection } from '../hex/selection'
import { rowToThumb, thumbToRow } from '../hex/virtualScroll'
import { createCanvasLayers, HexRenderer } from '../hex/renderer'

const props = defineProps<{
  fileSize: bigint
  sourceKey: string
  sourceRevision: string
  page: ViewportPage | null
  bytesPerRow: BytesPerRow
  selection: ByteSelection | null
  matches: bigint[]
  matchLength?: number
  templateRange: ByteSelection | null
  editMode: boolean
  theme: ColorTheme
  navigateOffset?: bigint
}>()

const emit = defineEmits<{
  'request-page': [request: PageRequest]
  select: [selection: ByteSelection]
  'edit-request': [offset: bigint]
  'viewport-offset': [offset: bigint]
}>()

const root = ref<HTMLElement | null>(null)
const canvas = ref<HTMLCanvasElement | null>(null)
const renderedRevision = ref<string>()
const size = reactive({ width: 1, height: 1 })
const canvasWidth = ref(1)
let layout: HexLayout = createLayout(size.width, props.bytesPerRow)
let renderer: HexRenderer | null = null
let resizeObserver: ResizeObserver | null = null
let frame = 0
const scrollRow = ref(0n)
let generation = 0
let activeRequest: (PageRequest & { sourceKey: string; sourceRevision: string }) | null = null
let acceptedPage: ViewportPage | null = null
let acceptedSourceKey: string | null = null
let anchor: bigint | null = null
let dragging = false
let staticDirty = true
let contentDirty = true
let overlayDirty = true
let scrollbarPointerId: number | null = null
let scrollbarGrabOffset = 0

const THUMB_HEIGHT = 24

const totalRows = computed(() => props.fileSize === 0n ? 0n : (props.fileSize + BigInt(props.bytesPerRow) - 1n) / BigInt(props.bytesPerRow))
const thumbTop = computed(() => `${rowToThumb(scrollRow.value, totalRows.value, Math.max(0, size.height - THUMB_HEIGHT))}px`)
const horizontalOverflow = computed(() => canvasWidth.value + 8 > size.width)

const THEMES = {
  dark: { background: '#111418', text: '#c9d1d9', address: '#8b949e', divider: '#30363d' },
  light: { background: '#ffffff', text: '#1f2328', address: '#57606a', divider: '#d0d7de' },
} as const

function invalidateAcceptedPage(): void {
  acceptedPage = null
  acceptedSourceKey = null
  renderedRevision.value = undefined
  contentDirty = true
  overlayDirty = true
}

function visibleByteInterval(): { start: bigint; end: bigint } {
  const start = scrollRow.value * BigInt(props.bytesPerRow)
  if (start >= props.fileSize) return { start, end: start }
  const rows = Math.max(1, Math.ceil(Math.max(0, size.height - layout.headerHeight) / layout.rowHeight))
  const requestedEnd = start + BigInt(rows * props.bytesPerRow)
  return { start, end: requestedEnd < props.fileSize ? requestedEnd : props.fileSize }
}

function acceptedPageCoversViewport(): boolean {
  if (!acceptedPage || acceptedSourceKey !== props.sourceKey || acceptedPage.revision !== props.sourceRevision) return false
  const interval = visibleByteInterval()
  const pageStart = BigInt(acceptedPage.offset)
  const pageEnd = pageStart + BigInt(acceptedPage.bytes.length)
  return pageStart <= interval.start && pageEnd >= interval.end
}

function requestPage(): void {
  if (acceptedPageCoversViewport()) {
    activeRequest = null
    return
  }
  const range = visibleRange(layout, scrollRow.value, size.height, props.fileSize)
  if (activeRequest && activeRequest.offset === range.byteStart && activeRequest.length === range.byteLength &&
      activeRequest.sourceKey === props.sourceKey && activeRequest.sourceRevision === props.sourceRevision) return
  const request = {
    offset: range.byteStart,
    length: range.byteLength,
    generation: ++generation,
    sourceKey: props.sourceKey,
    sourceRevision: props.sourceRevision,
  }
  activeRequest = request
  emit('request-page', { offset: request.offset, length: request.length, generation: request.generation })
}

function schedule(): void {
  if (frame) return
  let completedSynchronously = false
  const requestedFrame = requestAnimationFrame(() => {
    completedSynchronously = true
    frame = 0
    if (!renderer) return
    renderer.setViewport(layout, props.fileSize, scrollRow.value)
    if (staticDirty) {
      renderer.drawStatic()
      staticDirty = false
    }
    if (contentDirty) {
      renderer.drawContent(acceptedPage, scrollRow.value)
      contentDirty = false
    }
    if (overlayDirty) {
      renderer.drawOverlay({
        modifiedOffsets: new Set(acceptedPage?.modifiedOffsets ?? []),
        selection: props.selection,
        matches: props.matches,
        matchLength: props.matchLength ?? 1,
        templateRange: props.templateRange,
        viewportRow: scrollRow.value,
      })
      overlayDirty = false
    }
    renderer.composite()
  })
  frame = completedSynchronously ? 0 : requestedFrame
}

function rebuildLayout(width: number): void {
  const base = createLayout(Math.max(0, width - 8), props.bytesPerRow)
  layout = createLayout(Math.max(base.width, contentWidth(base)), props.bytesPerRow)
  canvasWidth.value = layout.width
  renderer?.resize(layout.width, size.height, globalThis.devicePixelRatio || 1)
}

function acceptPage(page: ViewportPage | null): void {
  if (!page) {
    invalidateAcceptedPage()
    schedule()
    return
  }
  if (!activeRequest || page.generation !== activeRequest.generation) return
  if (activeRequest.sourceKey !== props.sourceKey || activeRequest.sourceRevision !== props.sourceRevision) return
  if (BigInt(page.offset) !== activeRequest.offset || page.bytes.length > activeRequest.length) return
  if (page.revision !== props.sourceRevision) return
  acceptedPage = page
  acceptedSourceKey = props.sourceKey
  activeRequest = null
  renderedRevision.value = page.revision
  contentDirty = true
  overlayDirty = true
  schedule()
}

function resize(width: number, height: number): void {
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return
  size.width = width
  size.height = height
  rebuildLayout(width)
  staticDirty = contentDirty = overlayDirty = true
  requestPage()
  schedule()
}

function eventOffset(event: MouseEvent | PointerEvent): bigint | null {
  if (!canvas.value) return null
  const rect = canvas.value.getBoundingClientRect()
  return hitTestByte(layout, event.clientX - rect.left, event.clientY - rect.top, scrollRow.value * BigInt(props.bytesPerRow), props.fileSize)
}

function onPointerDown(event: PointerEvent): void {
  const offset = eventOffset(event)
  if (offset === null || !canvas.value) return
  dragging = true
  anchor = offset
  canvas.value.setPointerCapture(event.pointerId)
  emit('select', normalizeSelection(offset, offset))
}

function onPointerMove(event: PointerEvent): void {
  if (!dragging || anchor === null) return
  const offset = eventOffset(event)
  if (offset !== null) emit('select', normalizeSelection(anchor, offset))
}

function onPointerUp(event: PointerEvent): void {
  if (!dragging) return
  onPointerMove(event)
  dragging = false
  anchor = null
  canvas.value?.releasePointerCapture(event.pointerId)
}

function onDoubleClick(event: MouseEvent): void {
  if (!props.editMode) return
  const offset = eventOffset(event)
  if (offset !== null) emit('edit-request', offset)
}

function setScrollRow(row: bigint): void {
  const lastRow = totalRows.value > 0n ? totalRows.value - 1n : 0n
  scrollRow.value = row < 0n ? 0n : row > lastRow ? lastRow : row
  contentDirty = overlayDirty = true
  emit('viewport-offset', scrollRow.value * BigInt(props.bytesPerRow))
  requestPage()
  schedule()
}

function onWheel(event: WheelEvent): void {
  event.preventDefault()
  if (event.deltaY === 0) return
  setScrollRow(scrollRow.value + (event.deltaY > 0 ? 1n : -1n))
}

function updateScrollbarPointer(event: PointerEvent): void {
  const element = event.currentTarget as HTMLElement
  const rect = element.getBoundingClientRect()
  const trackHeight = Math.max(0, rect.height - THUMB_HEIGHT)
  setScrollRow(thumbToRow(event.clientY - rect.top - scrollbarGrabOffset, totalRows.value, trackHeight))
}

function onScrollbarPointerDown(event: PointerEvent): void {
  const element = event.currentTarget as HTMLElement
  const rect = element.getBoundingClientRect()
  const pointerY = event.clientY - rect.top
  const trackHeight = Math.max(0, rect.height - THUMB_HEIGHT)
  const currentTop = rowToThumb(scrollRow.value, totalRows.value, trackHeight)
  scrollbarGrabOffset = pointerY >= currentTop && pointerY <= currentTop + THUMB_HEIGHT
    ? pointerY - currentTop
    : THUMB_HEIGHT / 2
  scrollbarPointerId = event.pointerId
  element.setPointerCapture?.(event.pointerId)
  updateScrollbarPointer(event)
}

function onScrollbarPointerMove(event: PointerEvent): void {
  if (scrollbarPointerId !== event.pointerId || event.buttons !== 1) return
  updateScrollbarPointer(event)
}

function onScrollbarPointerUp(event: PointerEvent): void {
  if (scrollbarPointerId !== event.pointerId) return
  const element = event.currentTarget as HTMLElement
  element.releasePointerCapture?.(event.pointerId)
  scrollbarPointerId = null
}

watch(() => props.page, acceptPage)
watch(() => props.bytesPerRow, (nextWidth, previousWidth) => {
  const offset = scrollRow.value * BigInt(previousWidth)
  rebuildLayout(size.width)
  scrollRow.value = offset / BigInt(nextWidth)
  staticDirty = contentDirty = overlayDirty = true
  emit('viewport-offset', scrollRow.value * BigInt(nextWidth))
  requestPage()
  schedule()
})
watch(() => props.fileSize, () => {
  if (scrollRow.value >= totalRows.value) scrollRow.value = totalRows.value > 0n ? totalRows.value - 1n : 0n
  staticDirty = contentDirty = overlayDirty = true
  requestPage()
  schedule()
})
watch([() => props.sourceKey, () => props.sourceRevision], ([nextKey], [previousKey]) => {
  invalidateAcceptedPage()
  if (nextKey !== previousKey) scrollRow.value = 0n
  requestPage()
  schedule()
})
watch([() => props.selection, () => props.matches, () => props.matchLength, () => props.templateRange], () => {
  overlayDirty = true
  schedule()
}, { deep: true })
watch(() => props.theme, (theme) => {
  renderer?.setTheme(THEMES[theme])
  staticDirty = true
  contentDirty = true
  schedule()
})
watch(() => props.navigateOffset, (offset) => {
  if (offset === undefined || offset < 0n || offset >= props.fileSize) return
  const row = offset / BigInt(props.bytesPerRow)
  if (row !== scrollRow.value) setScrollRow(row)
})

onMounted(async () => {
  await nextTick()
  if (!canvas.value || !root.value) return
  const layers = createCanvasLayers(canvas.value)
  renderer = new HexRenderer(layers, { width: size.width, height: size.height, dpr: globalThis.devicePixelRatio || 1, layout, fileSize: props.fileSize })
  renderer.setTheme(THEMES[props.theme])
  resizeObserver = new ResizeObserver((entries) => {
    const box = entries[0]?.contentRect
    if (box) resize(box.width, box.height)
  })
  resizeObserver.observe(root.value)
  requestPage()
  schedule()
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  if (frame) cancelAnimationFrame(frame)
})
</script>

<template>
  <div ref="root" class="hex-canvas" data-testid="hex-canvas" :data-theme="theme" :style="{ '--canvas-width': `${canvasWidth}px`, overflowX: horizontalOverflow ? 'auto' : 'hidden' }">
    <canvas
      ref="canvas"
      :data-page-revision="renderedRevision"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
      @dblclick="onDoubleClick"
      @wheel="onWheel"
    />
    <div class="virtual-scrollbar" aria-label="Hex viewport scrollbar" @pointerdown="onScrollbarPointerDown" @pointermove="onScrollbarPointerMove" @pointerup="onScrollbarPointerUp" @pointercancel="onScrollbarPointerUp">
      <div class="virtual-scrollbar__thumb" :style="{ transform: `translateY(${thumbTop})` }" />
    </div>
  </div>
</template>

<style scoped>
.hex-canvas { display: grid; grid-template-columns: var(--canvas-width) 8px; min-width: 0; min-height: 0; overflow-y: hidden; }
canvas { display: block; width: 100%; height: 100%; cursor: default; }
.virtual-scrollbar { position: sticky; right: 0; background: color-mix(in srgb, currentColor 8%, transparent); touch-action: none; }
.virtual-scrollbar__thumb { position: absolute; inset: 0 1px auto; height: 24px; border-radius: 4px; background: color-mix(in srgb, currentColor 35%, transparent); }
</style>
