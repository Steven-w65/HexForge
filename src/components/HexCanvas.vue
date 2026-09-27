<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, shallowRef, watch } from 'vue'
import type { ColorTheme, ModifiedOverview, PageRequest, ParsedField, ViewportPage } from '../types'
import { contentWidth, createLayout, hitTestByte, visibleRange, type BytesPerRow, type HexLayout } from '../hex/layout'
import { normalizeSelection, type ByteSelection } from '../hex/selection'
import { offsetToOverviewPixel, rowToThumb, thumbToRow } from '../hex/virtualScroll'
import { createCanvasLayers, HexRenderer } from '../hex/renderer'
import { minimapPreview, minimapRowAt, minimapViewport, paintMinimap } from '../hex/minimap'

const props = defineProps<{
  fileSize: bigint
  sourceIdentity: number
  sourceKey: string
  sourceRevision: string
  page: ViewportPage | null
  bytesPerRow: BytesPerRow
  selection: ByteSelection | null
  matches: bigint[]
  modifiedOverview?: ModifiedOverview
  templateFields?: ParsedField[]
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
const minimapCanvas = ref<HTMLCanvasElement | null>(null)
const previewPage = shallowRef<ViewportPage | null>(null)
const renderedRevision = ref<string>()
const size = reactive({ width: 1, height: 1 })
const canvasWidth = ref(1)
let layout: HexLayout = createLayout(size.width, props.bytesPerRow)
let renderer: HexRenderer | null = null
let minimapContext: CanvasRenderingContext2D | null = null
let resizeObserver: ResizeObserver | null = null
let frame = 0
let measured = false
const scrollRow = ref(0n)
let generation = 0
let activeRequest: (PageRequest & { sourceIdentity: number; sourceKey: string; sourceRevision: string }) | null = null
let acceptedPage: ViewportPage | null = null
let acceptedSourceIdentity: number | null = null
let acceptedSourceKey: string | null = null
let anchor: bigint | null = null
let dragging = false
let staticDirty = true
let contentDirty = true
let overlayDirty = true
let minimapDirty = true
let scrollbarPointerId: number | null = null
let scrollbarGrabOffset = 0
let minimapPointerId: number | null = null

const THUMB_HEIGHT = 24
const MINIMAP_PREVIEW_WIDTH = 84
const SCROLL_STRIP_WIDTH = 12
const MINIMAP_WIDTH = MINIMAP_PREVIEW_WIDTH + SCROLL_STRIP_WIDTH

const totalRows = computed(() => props.fileSize === 0n ? 0n : (props.fileSize + BigInt(props.bytesPerRow) - 1n) / BigInt(props.bytesPerRow))
const visibleRows = computed(() => Math.max(1, Math.ceil(Math.max(0, size.height - layout.headerHeight) / layout.rowHeight)))
const fullyVisibleRows = computed(() => Math.max(1, Math.floor(Math.max(0, size.height - layout.headerHeight) / layout.rowHeight)))
const maxScrollRow = computed(() => totalRows.value > BigInt(fullyVisibleRows.value) ? totalRows.value - BigInt(fullyVisibleRows.value) : 0n)
const thumbHeight = computed(() => {
  const height = Math.max(1, Math.floor(size.height))
  if (totalRows.value <= BigInt(fullyVisibleRows.value)) return height
  const proportional = Number(BigInt(height) * BigInt(fullyVisibleRows.value) / totalRows.value)
  return Math.min(height, Math.max(THUMB_HEIGHT, proportional))
})
const thumbTop = computed(() => `${rowToThumb(scrollRow.value, maxScrollRow.value + 1n, Math.max(0, size.height - thumbHeight.value))}px`)
const horizontalOverflow = computed(() => canvasWidth.value + MINIMAP_WIDTH > size.width)
const preview = computed(() => minimapPreview(previewPage.value, props.bytesPerRow, size.height))
const previewViewport = computed(() => minimapViewport(preview.value, scrollRow.value, fullyVisibleRows.value))
const searchMarks = computed(() => {
  const pixels = new Set<number>()
  for (const offset of props.matches) {
    if (offset >= 0n && offset < props.fileSize) pixels.add(offsetToOverviewPixel(offset, props.fileSize, size.height))
  }
  return [...pixels].sort((a, b) => a - b)
})
const modifiedMarks = computed(() => {
  const { binCount, bins } = props.modifiedOverview ?? { binCount: 0, bins: [] }
  if (binCount < 2) return []
  const lastPixel = Math.max(0, Math.floor(size.height) - 1)
  const pixels = new Set<number>()
  for (const bin of bins) {
    if (Number.isInteger(bin) && bin >= 0 && bin < binCount) {
      pixels.add(Math.floor(bin * lastPixel / (binCount - 1)))
    }
  }
  return [...pixels].sort((a, b) => a - b)
})
const templateMarks = computed(() => {
  const ranges = new Map<number, number>()
  for (const field of props.templateFields ?? []) {
    try {
      const start = BigInt(field.offset)
      if (start < 0n || start >= props.fileSize || field.length <= 0) continue
      const end = start + BigInt(field.length) - 1n
      const top = offsetToOverviewPixel(start, props.fileSize, size.height)
      const bottom = offsetToOverviewPixel(end, props.fileSize, size.height)
      ranges.set(top, Math.max(ranges.get(top) ?? 0, Math.max(2, bottom - top + 1)))
    } catch { /* Ignore incomplete template draft ranges. */ }
  }
  return [...ranges].map(([top, height]) => ({ top, height }))
})

const THEMES = {
  dark: { background: '#111418', text: '#c9d1d9', address: '#8b949e', divider: '#30363d' },
  light: { background: '#ffffff', text: '#1f2328', address: '#57606a', divider: '#d0d7de' },
} as const

function invalidateAcceptedPage(): void {
  acceptedPage = null
  previewPage.value = null
  acceptedSourceIdentity = null
  acceptedSourceKey = null
  renderedRevision.value = undefined
  contentDirty = true
  overlayDirty = true
  minimapDirty = true
}

function visibleByteInterval(): { start: bigint; end: bigint } {
  const start = scrollRow.value * BigInt(props.bytesPerRow)
  if (start >= props.fileSize) return { start, end: start }
  const requestedEnd = start + BigInt(visibleRows.value * props.bytesPerRow)
  return { start, end: requestedEnd < props.fileSize ? requestedEnd : props.fileSize }
}

function acceptedPageCoversViewport(): boolean {
  if (!acceptedPage || acceptedSourceIdentity !== props.sourceIdentity || acceptedSourceKey !== props.sourceKey || acceptedPage.revision !== props.sourceRevision) return false
  const interval = visibleByteInterval()
  const pageStart = BigInt(acceptedPage.offset)
  const pageEnd = pageStart + BigInt(acceptedPage.bytes.length)
  return pageStart <= interval.start && pageEnd >= interval.end
}

function requestPage(): void {
  if (!measured) return
  if (acceptedPageCoversViewport()) {
    activeRequest = null
    return
  }
  const range = visibleRange(layout, scrollRow.value, size.height, props.fileSize, Math.ceil(size.height / 5))
  if (activeRequest && activeRequest.offset === range.byteStart && activeRequest.length === range.byteLength &&
      activeRequest.sourceIdentity === props.sourceIdentity && activeRequest.sourceKey === props.sourceKey && activeRequest.sourceRevision === props.sourceRevision) return
  const request = {
    offset: range.byteStart,
    length: range.byteLength,
    generation: ++generation,
    sourceIdentity: props.sourceIdentity,
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
    if (minimapDirty && minimapContext) {
      paintMinimap(minimapContext, previewPage.value, preview.value, props.bytesPerRow, MINIMAP_PREVIEW_WIDTH, size.height, props.theme)
      minimapDirty = false
    }
  })
  frame = completedSynchronously ? 0 : requestedFrame
}

function rebuildLayout(width: number): void {
  const base = createLayout(Math.max(0, width - MINIMAP_WIDTH), props.bytesPerRow)
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
  if (activeRequest.sourceIdentity !== props.sourceIdentity || activeRequest.sourceKey !== props.sourceKey || activeRequest.sourceRevision !== props.sourceRevision) return
  if (BigInt(page.offset) !== activeRequest.offset || page.bytes.length > activeRequest.length) return
  if (page.revision !== props.sourceRevision) return
  acceptedPage = page
  previewPage.value = page
  acceptedSourceIdentity = props.sourceIdentity
  acceptedSourceKey = props.sourceKey
  activeRequest = null
  renderedRevision.value = page.revision
  contentDirty = true
  overlayDirty = true
  minimapDirty = true
  schedule()
}

function resize(width: number, height: number): void {
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return
  size.width = width
  size.height = height
  measured = true
  rebuildLayout(width)
  if (minimapCanvas.value && minimapContext) {
    const dpr = globalThis.devicePixelRatio || 1
    minimapCanvas.value.width = Math.ceil(MINIMAP_PREVIEW_WIDTH * dpr)
    minimapCanvas.value.height = Math.ceil(height * dpr)
    minimapContext.setTransform(dpr, 0, 0, dpr, 0, 0)
  }
  minimapDirty = true
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
  scrollRow.value = row < 0n ? 0n : row > maxScrollRow.value ? maxScrollRow.value : row
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
  const trackHeight = Math.max(0, rect.height - thumbHeight.value)
  setScrollRow(thumbToRow(event.clientY - rect.top - scrollbarGrabOffset, maxScrollRow.value + 1n, trackHeight))
}

function onScrollbarPointerDown(event: PointerEvent): void {
  const element = event.currentTarget as HTMLElement
  const rect = element.getBoundingClientRect()
  const pointerY = event.clientY - rect.top
  const trackHeight = Math.max(0, rect.height - thumbHeight.value)
  const currentTop = rowToThumb(scrollRow.value, maxScrollRow.value + 1n, trackHeight)
  scrollbarGrabOffset = pointerY >= currentTop && pointerY <= currentTop + thumbHeight.value
    ? pointerY - currentTop
    : thumbHeight.value / 2
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

function updateMinimapPointer(event: PointerEvent): void {
  const current = preview.value
  const element = minimapCanvas.value
  if (!current || !element) return
  const rect = element.getBoundingClientRect()
  const targetRow = minimapRowAt(current, event.clientY - rect.top)
  setScrollRow(targetRow - BigInt(Math.floor(fullyVisibleRows.value / 2)))
}

function onMinimapPointerDown(event: PointerEvent): void {
  if (!preview.value || !minimapCanvas.value) return
  minimapPointerId = event.pointerId
  minimapCanvas.value.setPointerCapture?.(event.pointerId)
  updateMinimapPointer(event)
}

function onMinimapPointerMove(event: PointerEvent): void {
  if (minimapPointerId === event.pointerId && event.buttons === 1) updateMinimapPointer(event)
}

function onMinimapPointerUp(event: PointerEvent): void {
  if (minimapPointerId !== event.pointerId) return
  minimapCanvas.value?.releasePointerCapture?.(event.pointerId)
  minimapPointerId = null
}

watch(() => props.page, acceptPage)
watch(() => props.bytesPerRow, (nextWidth, previousWidth) => {
  const offset = scrollRow.value * BigInt(previousWidth)
  rebuildLayout(size.width)
  const nextRow = offset / BigInt(nextWidth)
  scrollRow.value = nextRow > maxScrollRow.value ? maxScrollRow.value : nextRow
  staticDirty = contentDirty = overlayDirty = true
  minimapDirty = true
  emit('viewport-offset', scrollRow.value * BigInt(nextWidth))
  requestPage()
  schedule()
})
watch(() => props.fileSize, () => {
  if (scrollRow.value > maxScrollRow.value) scrollRow.value = maxScrollRow.value
  staticDirty = contentDirty = overlayDirty = true
  requestPage()
  schedule()
})
watch([() => props.sourceIdentity, () => props.sourceKey, () => props.sourceRevision], ([nextIdentity, nextKey], [previousIdentity, previousKey]) => {
  invalidateAcceptedPage()
  if (nextIdentity !== previousIdentity || nextKey !== previousKey) {
    const requestedRow = (props.navigateOffset ?? 0n) / BigInt(props.bytesPerRow)
    scrollRow.value = requestedRow < 0n ? 0n : requestedRow > maxScrollRow.value ? maxScrollRow.value : requestedRow
  }
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
  minimapDirty = true
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
  minimapContext = minimapCanvas.value?.getContext('2d') ?? null
  renderer = new HexRenderer(layers, { width: size.width, height: size.height, dpr: globalThis.devicePixelRatio || 1, layout, fileSize: props.fileSize })
  renderer.setTheme(THEMES[props.theme])
  resizeObserver = new ResizeObserver((entries) => {
    const box = entries[0]?.contentRect
    if (box) resize(box.width, box.height)
  })
  resizeObserver.observe(root.value)
  schedule()
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  if (frame) cancelAnimationFrame(frame)
})
</script>

<template>
  <div ref="root" class="hex-canvas" data-testid="hex-canvas" :data-theme="theme" :style="{ '--canvas-width': `${canvasWidth}px`, width: '100%', height: '100%', overflowX: horizontalOverflow ? 'auto' : 'hidden' }">
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
    <div class="minimap-content" aria-label="Nearby hex preview">
      <canvas ref="minimapCanvas" data-testid="minimap-preview" @pointerdown="onMinimapPointerDown" @pointermove="onMinimapPointerMove"
        @pointerup="onMinimapPointerUp" @pointercancel="onMinimapPointerUp" />
      <div v-if="previewViewport" class="minimap-viewport" :style="{ top: `${previewViewport.top}px`, height: `${previewViewport.height}px` }" />
    </div>
    <div class="virtual-scrollbar" aria-label="Full-file scrollbar" @pointerdown="onScrollbarPointerDown" @pointermove="onScrollbarPointerMove" @pointerup="onScrollbarPointerUp" @pointercancel="onScrollbarPointerUp">
      <div class="minimap__markers">
        <span v-for="pixel in searchMarks" :key="pixel" class="minimap__match" :style="{ top: `${pixel}px` }" />
        <span v-for="mark in templateMarks" :key="mark.top" class="minimap__template" :style="{ top: `${mark.top}px`, height: `${mark.height}px` }" />
        <span v-for="pixel in modifiedMarks" :key="pixel" class="minimap__modified" :style="{ top: `${pixel}px` }" />
      </div>
      <div class="virtual-scrollbar__thumb" :style="{ transform: `translateY(${thumbTop})`, height: `${thumbHeight}px` }" />
    </div>
  </div>
</template>

<style scoped>
.hex-canvas { display: grid; grid-template-columns: var(--canvas-width) 84px 12px; min-width: 0; min-height: 0; overflow-y: hidden; }
canvas { display: block; width: 100%; height: 100%; cursor: default; }
.minimap-content { position: sticky; right: 12px; overflow: hidden; background: var(--panel); border-left: 1px solid var(--border); }
.minimap-content canvas { width: 84px; height: 100%; cursor: pointer; touch-action: none; }
.minimap-viewport { position: absolute; left: 0; right: 0; box-sizing: border-box; pointer-events: none; background: rgb(150 170 190 / 20%); border: 1px solid rgb(150 170 190 / 27%); }
.hex-canvas[data-theme='light'] .minimap-viewport { background: rgb(70 100 135 / 13%); border-color: rgb(70 100 135 / 22%); }
.virtual-scrollbar { position: sticky; right: 0; background: color-mix(in srgb, currentColor 8%, transparent); border-left: 1px solid var(--border); touch-action: none; cursor: pointer; }
.minimap__markers { position: absolute; inset: 0; pointer-events: none; }
.minimap__match, .minimap__template, .minimap__modified { position: absolute; left: 0; right: 0; min-height: 2px; border-radius: 1px; }
.minimap__match { height: 2px; background: #bda64a; opacity: .85; }
.minimap__template { background: #39c5cf; opacity: .7; }
.minimap__modified { height: 2px; background: #f0883e; }
.virtual-scrollbar__thumb { position: absolute; inset: 0 1px auto; height: 24px; border: 1px solid color-mix(in srgb, #1f6feb 75%, transparent); border-radius: 2px; background: color-mix(in srgb, #1f6feb 23%, transparent); pointer-events: none; }
</style>
