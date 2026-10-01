<script setup lang="ts">
import { ref } from 'vue'
const props = defineProps<{ modelValue: number; minimum: number; maximum: number }>()
const emit = defineEmits<{ 'update:modelValue': [height: number]; commit: [] }>()
const dragging = ref(false)
let anchor: { id: number; y: number; height: number } | null = null
function clamp(value: number): number { return Math.round(Math.min(props.maximum, Math.max(props.minimum, value))) }
function start(event: PointerEvent): void {
  if (event.button !== 0 || anchor) return
  event.preventDefault()
  anchor = { id: event.pointerId, y: event.clientY, height: props.modelValue }
  dragging.value = true
  ;(event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId)
}
function move(event: PointerEvent): void {
  if (!anchor || event.pointerId !== anchor.id) return
  // Moving the separator up increases the bottom pane, never the canvas.
  emit('update:modelValue', clamp(anchor.height + anchor.y - event.clientY))
}
function finish(event: PointerEvent): void {
  if (!anchor || event.pointerId !== anchor.id) return
  const element = event.currentTarget as HTMLElement
  anchor = null; dragging.value = false
  if (element.hasPointerCapture?.(event.pointerId)) element.releasePointerCapture(event.pointerId)
  emit('commit')
}
function keydown(event: KeyboardEvent): void {
  const step = event.shiftKey ? 5 : 20
  const height = event.key === 'ArrowUp' ? props.modelValue + step : event.key === 'ArrowDown' ? props.modelValue - step
    : event.key === 'Home' ? props.minimum : event.key === 'End' ? props.maximum : null
  if (height === null) return
  event.preventDefault(); event.stopPropagation()
  emit('update:modelValue', clamp(height)); emit('commit')
}
</script>

<template>
  <div class="results-splitter" :class="{ dragging }" role="separator" aria-label="Resize parsed results" aria-orientation="horizontal"
    :aria-valuemin="minimum" :aria-valuemax="maximum" :aria-valuenow="modelValue" :aria-valuetext="`${modelValue} pixels`" tabindex="0"
    title="Drag to resize results. Arrow keys adjust; Home/End set limits."
    @pointerdown="start" @pointermove="move" @pointerup="finish" @pointercancel="finish" @lostpointercapture="finish" @keydown="keydown" />
</template>

<style scoped>
.results-splitter { position: relative; z-index: 1; height: 4px; background: var(--bg); cursor: row-resize; touch-action: none; user-select: none; }
.results-splitter::before { content: ''; position: absolute; inset: -3px 0; }
.results-splitter:hover, .results-splitter.dragging, .results-splitter:focus-visible { background: var(--selection); outline: none; }
</style>
