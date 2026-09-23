<script setup lang="ts">
import type { Endian } from '../types'
import type { BytesPerRow } from '../hex/layout'

defineProps<{
  fileSize: bigint; selected: { offset: bigint; value: number } | null; selectedCount: bigint
  bytesPerRow: BytesPerRow; editMode: boolean; endianness: Endian; dirty: boolean
  busyLabel?: string; progressText?: string
}>()
const emit = defineEmits<{ 'update:bytesPerRow': [value: BytesPerRow] }>()

function formatSize(bytes: bigint): string {
  if (bytes < 1024n) return `${bytes} B`
  if (bytes < 1024n ** 2n) return `${Number(bytes) / 1024} KiB`
  if (bytes < 1024n ** 3n) return `${(Number(bytes) / 1024 ** 2).toFixed(2)} MiB`
  return `${(Number(bytes) / 1024 ** 3).toFixed(2)} GiB`
}

function ascii(value: number): string { return value >= 0x20 && value <= 0x7e ? String.fromCharCode(value) : '.' }
</script>

<template>
  <footer class="status-bar">
    <span>Size {{ formatSize(fileSize) }}</span>
    <span>Offset {{ selected ? `0x${selected.offset.toString(16).toUpperCase()}` : '—' }}</span>
    <span>Byte {{ selected ? `${selected.value.toString(16).padStart(2, '0').toUpperCase()} '${ascii(selected.value)}'` : '—' }}</span>
    <span>Selected {{ selectedCount.toString() }}</span>
    <span class="row-width">Rows <button type="button" data-row-width="16" :class="{ active: bytesPerRow === 16 }" @click="emit('update:bytesPerRow', 16)">16</button>/<button type="button" data-row-width="32" :class="{ active: bytesPerRow === 32 }" @click="emit('update:bytesPerRow', 32)">32</button></span>
    <span>{{ editMode ? 'Edit' : 'Read-only' }}</span>
    <span>Parse: {{ endianness === 'little' ? 'LE' : 'BE' }}</span>
    <span v-if="busyLabel" data-testid="status-progress" class="progress" role="status">{{ busyLabel }}<template v-if="progressText"> · {{ progressText }}</template></span>
    <span :class="{ modified: dirty }">{{ dirty ? 'Modified' : 'Clean' }}</span>
  </footer>
</template>

<style scoped>
.status-bar { height: 24px; display: flex; align-items: center; gap: 0; overflow: hidden; color: var(--muted); background: var(--surface); border-top: 1px solid var(--border); font-size: 10px; white-space: nowrap; }
.status-bar > span { padding: 0 9px; border-right: 1px solid var(--border); }
.status-bar > span:last-child { margin-left: auto; border: 0; }
.progress { overflow: hidden; color: var(--text); text-overflow: ellipsis; }
button { padding: 1px 3px; color: var(--muted); background: transparent; border: 0; border-radius: 2px; font: inherit; }
button:hover, button.active { color: var(--text); background: var(--hover); }
.modified { color: var(--modified); }
</style>
