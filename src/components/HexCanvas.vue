<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { PageRequest, ViewportPage } from '../types'
import { createLayout, hitTestByte, visibleRange, type BytesPerRow, type HexLayout } from '../hex/layout'
import { normalizeSelection, type ByteSelection } from '../hex/selection'
import { rowToThumb, thumbToRow } from '../hex/virtualScroll'
import { createCanvasLayers, HexRenderer } from '../hex/renderer'

const props = defineProps<{
  fileSize: bigint
  page: ViewportPage | null
  bytesPerRow: BytesPerRow
  selection: ByteSelection | null
  matches: bigint[]
  templateRange: ByteSelection | null
  editMode: boolean
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
const size = { width: 1, height: 1 }
let layout: HexLayout = createLayout(size.width, props.bytesPerRow)
let renderer: HexRenderer | null = null
let resizeObserver: ResizeObserver | null = null
let frame = 0
let scrollRow = 0n
let generation = 0
let activeRequest: PageRequest | null = null
let acceptedPage: ViewportPage | null = null
let anchor: bigint | null = null
let dragging = false
let staticDirty = true
let contentDirty = true
let overlayDirty = true

const totalRows = computed(() => props.fileSize === 0n ? 0n : (props.fileSize + BigInt(props.bytesPerRow) - 1n) / BigInt(props.bytesPerRow))
const thumbTop = computed(() => `${rowToThumb(scrollRow, totalRows.value, Math.max(0, size.height - 24))}px`)

function requestPage(): void {
  const range = visibleRange(layout, scrollRow, size.height, props.fileSize)
  const request = { offset: range.byteStart, length: range.byteLength, generation: ++generation }
  activeRequest = request
  emit('request-page', request)
}

function schedule(): void {
  if (frame) return
  frame = requestAnimationFrame(() => {
    frame = 0
    if (!renderer) return
    renderer.setViewport(layout, props.fileSize, scrollRow)
    if (staticDirty) {
      renderer.drawStatic()
      staticDirty = false
    }
    if (contentDirty) {
      if (acceptedPage) renderer.drawContent(acceptedPage, scrollRow)
      contentDirty = false
    }
    if (overlayDirty) {
      renderer.drawOverlay({
        modifiedOffsets: new Set(acceptedPage?.modifiedOffsets ?? []),
        selection: props.selection,
        matches: props.matches,
        templateRange: props.templateRange,
        viewportRow: scrollRow,
      })
      overlayDirty = false
    }
    renderer.composite()
  })
}

function revisionIsOlder(next: string, current: string | undefined): boolean {
  if (current === undefined) return false
  try { return BigInt(next) < BigInt(current) } catch { return next < current }
}

function acceptPage(page: ViewportPage | null): void {
  if (!page || !activeRequest || page.generation !== activeRequest.generation) return
  if (BigInt(page.offset) !== activeRequest.offset || page.bytes.length > activeRequest.length) return
  if (revisionIsOlder(page.revision, renderedRevision.value)) return
  acceptedPage = page
  renderedRevision.value = page.revision
  contentDirty = true
  overlayDirty = true
  schedule()
}

function resize(width: number, height: number): void {
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return
  size.width = width
  size.height = height
  layout = createLayout(width, props.bytesPerRow)
  renderer?.resize(width, height, globalThis.devicePixelRatio || 1)
  staticDirty = contentDirty = overlayDirty = true
  requestPage()
  schedule()
}

function eventOffset(event: MouseEvent | PointerEvent): bigint | null {
  if (!canvas.value) return null
  const rect = canvas.value.getBoundingClientRect()
  return hitTestByte(layout, event.clientX - rect.left, event.clientY - rect.top, scrollRow * BigInt(props.bytesPerRow), props.fileSize)
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
  scrollRow = row < 0n ? 0n : row > lastRow ? lastRow : row
  contentDirty = overlayDirty = true
  emit('viewport-offset', scrollRow * BigInt(props.bytesPerRow))
  requestPage()
  schedule()
}

function onWheel(event: WheelEvent): void {
  event.preventDefault()
  if (event.deltaY === 0) return
  setScrollRow(scrollRow + (event.deltaY > 0 ? 1n : -1n))
}

function onScrollbarPointer(event: PointerEvent): void {
  const element = event.currentTarget as HTMLElement
  const rect = element.getBoundingClientRect()
  setScrollRow(thumbToRow(event.clientY - rect.top, totalRows.value, rect.height))
}

watch(() => props.page, acceptPage)
watch(() => props.bytesPerRow, () => {
  layout = createLayout(size.width, props.bytesPerRow)
  const offset = scrollRow * BigInt(props.bytesPerRow)
  scrollRow = offset / BigInt(props.bytesPerRow)
  staticDirty = contentDirty = overlayDirty = true
  requestPage()
  schedule()
})
watch(() => props.fileSize, () => {
  if (scrollRow >= totalRows.value) scrollRow = totalRows.value > 0n ? totalRows.value - 1n : 0n
  staticDirty = contentDirty = overlayDirty = true
  requestPage()
  schedule()
})
watch([() => props.selection, () => props.matches, () => props.templateRange], () => {
  overlayDirty = true
  schedule()
}, { deep: true })

onMounted(async () => {
  await nextTick()
  if (!canvas.value || !root.value) return
  const layers = createCanvasLayers(canvas.value)
  renderer = new HexRenderer(layers, { width: size.width, height: size.height, dpr: globalThis.devicePixelRatio || 1, layout, fileSize: props.fileSize })
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
  <div ref="root" class="hex-canvas" data-testid="hex-canvas">
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
    <div class="virtual-scrollbar" aria-label="Hex viewport scrollbar" @pointerdown="onScrollbarPointer" @pointermove="($event.buttons === 1) && onScrollbarPointer($event)">
      <div class="virtual-scrollbar__thumb" :style="{ transform: `translateY(${thumbTop})` }" />
    </div>
  </div>
</template>

<style scoped>
.hex-canvas { display: grid; grid-template-columns: minmax(0, 1fr) 8px; min-width: 0; min-height: 0; overflow: hidden; }
canvas { display: block; width: 100%; height: 100%; cursor: default; }
.virtual-scrollbar { position: relative; background: color-mix(in srgb, currentColor 8%, transparent); touch-action: none; }
.virtual-scrollbar__thumb { position: absolute; inset: 0 1px auto; height: 24px; border-radius: 4px; background: color-mix(in srgb, currentColor 35%, transparent); }
</style>
