<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import type { ColorTheme, ModifiedOverview, PageRequest, ParsedField, ViewportPage } from '../types'
import { backend } from '../api/backend'
import { contentWidth, createLayout, hitTestByte, visibleRange, type BytesPerRow, type HexLayout } from '../hex/layout'
import { normalizeSelection, type ByteSelection } from '../hex/selection'
import { rowToThumb, thumbToRow } from '../hex/virtualScroll'
import { createCanvasLayers, HexRenderer } from '../hex/renderer'
import { MINIMAP_SUBPIXELS, createMinimapGeometry, samplePlan, viewportBox, DEFAULT_MINIMAP_SETTINGS, type MinimapSettings } from '../hex/minimapGeometry'
import { createMinimapDataManager, type MinimapDataManager, type MinimapRow } from '../hex/minimapData'
import { collectMinimapMarkers, projectMinimapMarkers, projectOverviewMarkers } from '../hex/minimapMarkers'
import { createMinimapCanvasLayers, MinimapRenderer } from '../hex/minimapRenderer'
import { createMinimapInteractionController, type MinimapInteractionController } from '../hex/minimapInteraction'

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
  minimapSettings?: MinimapSettings
  editDelta?: { offset: bigint; revision: string } | null
}>()

const emit = defineEmits<{
  'request-page': [request: PageRequest]
  select: [selection: ByteSelection]
  'edit-request': [offset: bigint]
  'viewport-offset': [offset: bigint]
  'minimap-error': [error: unknown]
}>()

const root = ref<HTMLElement | null>(null)
const canvas = ref<HTMLCanvasElement | null>(null)
const gutterCanvas = ref<HTMLCanvasElement | null>(null)
const minimapCanvas = ref<HTMLCanvasElement | null>(null)
const minimapContent = ref<HTMLElement | null>(null)
const renderedRevision = ref<string>()
const size = reactive({ width: 1, height: 1 })
const canvasWidth = ref(1)
let layout: HexLayout = createLayout(size.width, props.bytesPerRow)
const gutterWidth = ref(layout.hexX - layout.charWidth + 1)
let renderer: HexRenderer | null = null
let gutterContext: CanvasRenderingContext2D | null = null
let miniRenderer: MinimapRenderer | null = null
let miniData: MinimapDataManager | null = null
let miniInteraction: MinimapInteractionController | null = null
let resizeObserver: ResizeObserver | null = null
let frame = 0
let minimapInputFrame = 0
let minimapInputGeneration = 0
let pendingMinimapMainRow: bigint | null = null
let applyingMinimapFrame = false
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
const visualMinimapTopPx = ref<number | null>(null)
const minimapHovered = ref(false)
const minimapDragging = ref(false)

const THUMB_HEIGHT = 24
const MINIMAP_PREVIEW_WIDTH = 84
const settings = computed(() => props.minimapSettings ?? DEFAULT_MINIMAP_SETTINGS)

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
const horizontalOverflow = computed(() => canvasWidth.value > size.width)
const miniGeometry = computed(() => createMinimapGeometry({
  mode: settings.value.mode, totalRows: totalRows.value, heightPx: Math.max(1, Math.floor(size.height)),
  rowPx: settings.value.scale * 2, topRow: scrollRow.value,
  visibleMainRows: BigInt(fullyVisibleRows.value), visualTopPx: visualMinimapTopPx.value,
}))
const markers = computed(() => collectMinimapMarkers(props.matches, props.matchLength ?? 1, props.templateFields ?? [],
  props.modifiedOverview ?? { binCount: 0, bins: [] }, props.fileSize, props.bytesPerRow))
const overviewMarks = computed(() => projectOverviewMarkers(markers.value, totalRows.value, Math.floor(size.height)))
const searchMarks = computed(() => overviewMarks.value.filter((mark) => mark.kind === 'search'))
const templateMarks = computed(() => overviewMarks.value.filter((mark) => mark.kind === 'template'))
const modifiedMarks = computed(() => overviewMarks.value.filter((mark) => mark.kind === 'modified'))

const THEMES = {
  dark: { background: '#111418', text: '#c9d1d9', address: '#8b949e', divider: '#30363d' },
  light: { background: '#ffffff', text: '#1f2328', address: '#57606a', divider: '#d0d7de' },
} as const

function invalidateAcceptedPage(): void {
  acceptedPage = null
  acceptedSourceIdentity = null
  acceptedSourceKey = null
  renderedRevision.value = undefined
  contentDirty = true
  overlayDirty = true
  minimapDirty = true
  syncMinimapData()
}

function syncMinimapData(): void {
  miniData?.update({
    enabled: settings.value.enabled,
    sourceIdentity: props.sourceIdentity, sourceKey: props.sourceKey, revision: props.sourceRevision,
    fileSize: props.fileSize, bytesPerRow: props.bytesPerRow, geometry: miniGeometry.value,
    acceptedPage: acceptedSourceIdentity === props.sourceIdentity && acceptedSourceKey === props.sourceKey &&
      acceptedPage?.revision === props.sourceRevision ? acceptedPage : null,
  })
}

function paintCurrentMinimap(): void {
  if (!settings.value.enabled || !miniRenderer) return
  const geometry = miniGeometry.value
  const rows = new Map<bigint, MinimapRow>()
  for (const position of samplePlan(geometry)) {
    const sample = miniData?.getRow(position.row)
    if (sample) rows.set(position.row, sample)
  }
  miniRenderer.resize(MINIMAP_PREVIEW_WIDTH, Math.max(1, geometry.canvasHeightPx), globalThis.devicePixelRatio || 1)
  if (minimapCanvas.value) minimapCanvas.value.style.height = `${geometry.canvasHeightPx}px`
  miniRenderer.setContent(geometry, rows, {
    bytesPerRow: props.bytesPerRow, renderCharacters: settings.value.renderCharacters,
    scale: settings.value.scale,
    theme: props.theme,
  })
  miniRenderer.setMarkers(projectMinimapMarkers(markers.value, geometry))
  miniRenderer.setViewport(viewportBox(geometry, scrollRow.value, scrollRow.value + BigInt(fullyVisibleRows.value)),
    geometry.mode === 'proportional' ? minimapDragging.value ? 'active' : minimapHovered.value ? 'hover' : 'normal' : 'normal')
  miniRenderer.paint()
}

function ensureMiniRenderer(): void {
  if (!settings.value.enabled || !minimapCanvas.value || miniRenderer) return
  try { miniRenderer = new MinimapRenderer(createMinimapCanvasLayers(minimapCanvas.value)) }
  catch (error) { emit('minimap-error', error) }
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
    if (gutterContext && gutterCanvas.value && canvas.value) {
      gutterContext.clearRect(0, 0, gutterWidth.value, size.height)
      gutterContext.drawImage(canvas.value, 0, 0, gutterCanvas.value.width, gutterCanvas.value.height, 0, 0, gutterWidth.value, size.height)
    }
    if (minimapDirty) {
      paintCurrentMinimap()
      minimapDirty = false
    }
  })
  frame = completedSynchronously ? 0 : requestedFrame
}

function rebuildLayout(width: number): void {
  const base = createLayout(Math.max(0, width), props.bytesPerRow)
  layout = createLayout(Math.max(base.width, contentWidth(base)), props.bytesPerRow)
  gutterWidth.value = layout.hexX - layout.charWidth + 1
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
  acceptedSourceIdentity = props.sourceIdentity
  acceptedSourceKey = props.sourceKey
  activeRequest = null
  renderedRevision.value = page.revision
  contentDirty = true
  overlayDirty = true
  minimapDirty = true
  syncMinimapData()
  schedule()
}

function resize(width: number, height: number): void {
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return
  size.width = width
  size.height = height
  measured = true
  rebuildLayout(width)
  if (gutterCanvas.value && gutterContext) {
    const dpr = globalThis.devicePixelRatio || 1
    gutterCanvas.value.width = Math.max(1, Math.round(gutterWidth.value * dpr))
    gutterCanvas.value.height = Math.max(1, Math.round(height * dpr))
    gutterContext.setTransform(dpr, 0, 0, dpr, 0, 0)
  }
  syncMinimapData()
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
  if (!applyingMinimapFrame && minimapInputFrame) cancelPendingMinimapInput()
  scrollRow.value = row < 0n ? 0n : row > maxScrollRow.value ? maxScrollRow.value : row
  contentDirty = overlayDirty = true
  emit('viewport-offset', scrollRow.value * BigInt(props.bytesPerRow))
  syncMinimapData()
  minimapDirty = true
  requestPage()
  schedule()
}

function cancelPendingMinimapInput(): void {
  minimapInputGeneration += 1
  if (minimapInputFrame) cancelAnimationFrame(minimapInputFrame)
  minimapInputFrame = 0
  pendingMinimapMainRow = null
}

function scheduleMinimapInput(): void {
  if (minimapInputFrame) return
  const generation = minimapInputGeneration
  let completedSynchronously = false
  const requestedFrame = requestAnimationFrame(() => {
    completedSynchronously = true
    if (generation !== minimapInputGeneration) return
    minimapInputFrame = 0
    const row = pendingMinimapMainRow
    pendingMinimapMainRow = null
    if (row !== null) {
      applyingMinimapFrame = true
      try { setScrollRow(row) } finally { applyingMinimapFrame = false }
    }
  })
  minimapInputFrame = completedSynchronously ? 0 : requestedFrame
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
  const element = minimapContent.value
  if (!element || !miniInteraction) return
  const rect = element.getBoundingClientRect()
  miniInteraction.dragViewport(event.clientY - rect.top)
}

function onMinimapPointerDown(event: PointerEvent): void {
  if (!minimapCanvas.value || !minimapContent.value || !miniInteraction) return
  const rect = minimapContent.value.getBoundingClientRect()
  const y = event.clientY - rect.top
  const geometry = miniGeometry.value
  if (geometry.totalRows === 0n) return
  const box = viewportBox(geometry, scrollRow.value, scrollRow.value + BigInt(fullyVisibleRows.value))
  const boxTop = geometry.mode === 'proportional'
    ? Number(geometry.viewportTopSubPx) / Number(MINIMAP_SUBPIXELS) : box?.top ?? 0
  const inBox = !!box && y >= boxTop && y < boxTop + box.height
  if (inBox || geometry.mode === 'fit') {
    minimapPointerId = event.pointerId
    minimapCanvas.value.setPointerCapture?.(event.pointerId)
  }
  if (inBox) {
    minimapDragging.value = geometry.mode === 'proportional'
    minimapHovered.value = true
    miniInteraction.beginViewportDrag(y, scrollRow.value)
    minimapDirty = true
    schedule()
  } else {
    miniInteraction.click(y)
    // Fit mode retains its existing click-then-drag behavior. A
    // proportional preview click is navigation only, never a box drag.
    if (geometry.mode === 'fit') miniInteraction.beginViewportDrag(y, pendingMinimapMainRow ?? scrollRow.value)
  }
}

function onMinimapPointerMove(event: PointerEvent): void {
  if (minimapPointerId !== event.pointerId) {
    if (settings.value.mode === 'proportional' && minimapPointerId === null && minimapContent.value) {
      const y = event.clientY - minimapContent.value.getBoundingClientRect().top
      const g = miniGeometry.value
      const top = Number(g.viewportTopSubPx) / Number(MINIMAP_SUBPIXELS)
      const hovered = y >= top && y < top + g.viewportHeightPx
      if (minimapHovered.value !== hovered) { minimapHovered.value = hovered; minimapDirty = true; schedule() }
    }
    return
  }
  if (event.buttons !== 1) return
  const rect = minimapContent.value?.getBoundingClientRect()
  // Fit retains its old exit behavior. Proportional capture continues and
  // clamps vertical movement outside the column to an endpoint.
  if (settings.value.mode === 'fit' && rect && rect.width > 0 && rect.height > 0 &&
      (event.clientX < rect.left || event.clientX >= rect.right || event.clientY < rect.top || event.clientY >= rect.bottom)) {
    onMinimapPointerUp(event)
    return
  }
  updateMinimapPointer(event)
}

function onMinimapPointerUp(event: PointerEvent): void {
  if (minimapPointerId !== event.pointerId) return
  minimapPointerId = null
  minimapDragging.value = false
  miniInteraction?.endDrag()
  if (settings.value.mode === 'proportional' && minimapContent.value) {
    const rect = minimapContent.value.getBoundingClientRect()
    const y = event.clientY - rect.top
    const g = miniGeometry.value
    const top = Number(g.viewportTopSubPx) / Number(MINIMAP_SUBPIXELS)
    minimapHovered.value = event.clientX >= rect.left && event.clientX < rect.right &&
      y >= top && y < top + g.viewportHeightPx
  }
  minimapDirty = true
  schedule()
  if (minimapCanvas.value?.hasPointerCapture?.(event.pointerId)) {
    minimapCanvas.value.releasePointerCapture(event.pointerId)
  }
}

function onMinimapPointerLeave(event: PointerEvent): void {
  minimapHovered.value = false
  minimapDirty = true
  schedule()
  if (settings.value.mode === 'fit') onMinimapPointerUp(event)
}

function onMinimapWheel(event: WheelEvent): void {
  event.preventDefault()
  miniInteraction?.wheel(event.deltaY)
}

watch(() => props.page, acceptPage)
watch(() => props.bytesPerRow, (nextWidth, previousWidth) => {
  const offset = scrollRow.value * BigInt(previousWidth)
  rebuildLayout(size.width)
  const nextRow = offset / BigInt(nextWidth)
  scrollRow.value = nextRow > maxScrollRow.value ? maxScrollRow.value : nextRow
  staticDirty = contentDirty = overlayDirty = true
  minimapDirty = true
  syncMinimapData()
  emit('viewport-offset', scrollRow.value * BigInt(nextWidth))
  requestPage()
  schedule()
})
watch(() => props.fileSize, () => {
  if (scrollRow.value > maxScrollRow.value) scrollRow.value = maxScrollRow.value
  staticDirty = contentDirty = overlayDirty = true
  minimapDirty = true
  syncMinimapData()
  requestPage()
  schedule()
})
watch([() => props.sourceIdentity, () => props.sourceKey, () => props.sourceRevision], ([nextIdentity, nextKey], [previousIdentity, previousKey]) => {
  invalidateAcceptedPage()
  if (nextIdentity !== previousIdentity || nextKey !== previousKey) {
    cancelPendingMinimapInput()
    miniInteraction?.endDrag()
    minimapPointerId = null
    if (root.value) root.value.scrollLeft = 0
    visualMinimapTopPx.value = null
    const requestedRow = (props.navigateOffset ?? 0n) / BigInt(props.bytesPerRow)
    scrollRow.value = requestedRow < 0n ? 0n : requestedRow > maxScrollRow.value ? maxScrollRow.value : requestedRow
  }
  syncMinimapData()
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
watch([() => props.minimapSettings, () => props.modifiedOverview, () => props.templateFields, () => props.matches], () => {
  void nextTick(() => {
    if (!settings.value.enabled && miniRenderer) { miniRenderer.dispose(); miniRenderer = null }
    ensureMiniRenderer()
    syncMinimapData()
    minimapDirty = true
    schedule()
  })
}, { deep: true })
watch(() => props.editDelta, (delta) => {
  if (delta) miniData?.applyEditDelta(delta)
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
  gutterContext = gutterCanvas.value?.getContext('2d') ?? null
  ensureMiniRenderer()
  miniData = createMinimapDataManager(backend,
    (rows) => { miniRenderer?.invalidateRows(rows); minimapDirty = true; schedule() },
    (error) => emit('minimap-error', error))
  miniInteraction = createMinimapInteractionController({
    geometry: () => miniGeometry.value, visibleMainRows: () => BigInt(fullyVisibleRows.value),
    getMainRow: () => pendingMinimapMainRow ?? scrollRow.value,
    setMainRow: (row) => { pendingMinimapMainRow = row; scheduleMinimapInput() },
    setVisualTop: (top) => {
      if (top === null && pendingMinimapMainRow !== null) {
        const row = pendingMinimapMainRow
        cancelPendingMinimapInput()
        setScrollRow(row)
      }
      visualMinimapTopPx.value = top
      syncMinimapData()
      minimapDirty = true
      schedule()
    },
  })
  renderer = new HexRenderer(layers, { width: size.width, height: size.height, dpr: globalThis.devicePixelRatio || 1, layout, fileSize: props.fileSize })
  renderer.setTheme(THEMES[props.theme])
  syncMinimapData()
  resizeObserver = new ResizeObserver((entries) => {
    const box = entries[0]?.contentRect
    if (box) resize(box.width, box.height)
  })
  resizeObserver.observe(root.value)
  schedule()
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  miniData?.dispose()
  miniRenderer?.dispose()
  if (frame) cancelAnimationFrame(frame)
  cancelPendingMinimapInput()
  miniInteraction?.endDrag()
})
</script>

<template>
  <div class="hex-frame">
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
    </div>
    <div v-if="settings.enabled" ref="minimapContent" class="minimap-content" aria-label="File minimap" :class="{ 'is-box-hover': minimapHovered, 'is-box-dragging': minimapDragging }" :style="{ height: '100%' }" @wheel="onMinimapWheel" @pointerleave="onMinimapPointerLeave">
      <canvas ref="minimapCanvas" data-testid="minimap-preview" :style="{ height: `${miniGeometry.canvasHeightPx}px` }"
        @pointerdown="onMinimapPointerDown" @pointermove="onMinimapPointerMove" @pointerup="onMinimapPointerUp" @pointercancel="onMinimapPointerUp"
        @lostpointercapture="onMinimapPointerUp"
      />
    </div>
    <div class="virtual-scrollbar" aria-label="Full-file scrollbar" @pointerdown="onScrollbarPointerDown" @pointermove="onScrollbarPointerMove" @pointerup="onScrollbarPointerUp" @pointercancel="onScrollbarPointerUp">
        <div class="minimap__markers">
          <span v-for="mark in searchMarks" :key="mark.top" class="minimap__match" :style="{ top: `${mark.top}px`, height: `${mark.height}px` }" />
          <span v-for="mark in templateMarks" :key="mark.top" class="minimap__template" :style="{ top: `${mark.top}px`, height: `${mark.height}px` }" />
          <span v-for="mark in modifiedMarks" :key="mark.top" class="minimap__modified" :style="{ top: `${mark.top}px`, height: `${mark.height}px` }" />
        </div>
        <div class="virtual-scrollbar__thumb" :style="{ transform: `translateY(${thumbTop})`, height: `${thumbHeight}px` }" />
    </div>
    <canvas ref="gutterCanvas" class="address-gutter" data-testid="address-gutter" aria-hidden="true"
      :style="{ width: `${gutterWidth}px`, height: `${size.height}px` }" @wheel="onWheel" />
  </div>
</template>

<style scoped>
.hex-frame { position: relative; display: flex; width: 100%; height: 100%; min-width: 0; min-height: 0; }
.hex-canvas { flex: 1 1 auto; min-width: 0; min-height: 0; overflow-y: hidden; }
.hex-canvas > canvas { width: var(--canvas-width); }
canvas { display: block; width: 100%; height: 100%; cursor: default; }
.address-gutter { position: absolute; z-index: 1; top: 0; left: 0; cursor: default; }
.minimap-content { position: relative; flex: 0 0 84px; overflow: hidden; background: var(--panel); border-left: 1px solid var(--border); }
.minimap-content canvas { width: 84px; cursor: pointer; touch-action: none; }
.minimap-content.is-box-hover canvas { cursor: grab; }
.minimap-content.is-box-dragging canvas { cursor: grabbing; }
.virtual-scrollbar { position: relative; flex: 0 0 12px; background: color-mix(in srgb, currentColor 8%, transparent); border-left: 1px solid var(--border); touch-action: none; cursor: pointer; }
.minimap__markers { position: absolute; inset: 0; pointer-events: none; }
.minimap__match, .minimap__template, .minimap__modified { position: absolute; left: 0; right: 0; min-height: 2px; border-radius: 1px; }
.minimap__match { height: 2px; background: #bda64a; opacity: .65; }
.minimap__template { background: #39c5cf; opacity: .55; }
.minimap__modified { height: 2px; background: #f0883e; opacity: .9; }
.virtual-scrollbar__thumb { position: absolute; inset: 0 1px auto; height: 24px; border: 1px solid color-mix(in srgb, currentColor 38%, transparent); border-radius: 2px; background: color-mix(in srgb, currentColor 17%, transparent); pointer-events: none; }
</style>
