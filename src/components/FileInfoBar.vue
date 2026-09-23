<script setup lang="ts">
import { ref, watch } from 'vue'
import type { FileInfo } from '../types'

const props = defineProps<{ file: FileInfo | null }>()
const expanded = ref(false)

watch(() => props.file?.path, () => { expanded.value = false })

function formatSize(value: string): string {
  const bytes = BigInt(value)
  if (bytes < 1024n) return `${bytes} B`
  if (bytes < 1024n ** 2n) return `${(Number(bytes) / 1024).toFixed(2)} KiB`
  if (bytes < 1024n ** 3n) return `${(Number(bytes) / 1024 ** 2).toFixed(2)} MiB`
  return `${(Number(bytes) / 1024 ** 3).toFixed(2)} GiB`
}
</script>

<template>
  <header data-testid="file-info-bar" class="file-info-bar" :class="{ expanded }">
    <template v-if="file">
      <div class="summary">
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 3h8l4 4v14H6z M14 3v5h5" /></svg>
        <strong :title="file.name">{{ file.name }}</strong>
        <span class="separator" aria-hidden="true">·</span>
        <span class="size">{{ formatSize(file.size) }}</span>
        <span class="separator" aria-hidden="true">·</span>
        <span data-testid="file-path" class="path" :title="file.path">{{ file.path }}</span>
        <span v-if="file.dirty" class="modified"><i aria-hidden="true" />Modified</span>
        <button type="button" data-action="toggle-file-details" :aria-expanded="expanded"
          :aria-label="expanded ? 'Hide full file details' : 'Show full file details'" @click="expanded = !expanded">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="expanded ? 'M7 14l5-5 5 5' : 'M7 10l5 5 5-5'" /></svg>
        </button>
      </div>
      <div v-if="expanded" data-testid="file-details" class="details">
        <span class="details-label">PATH</span><span class="full-path">{{ file.path }}</span>
        <span class="details-label">SIZE</span><span>{{ file.size }} bytes</span>
      </div>
    </template>
    <div v-else class="summary empty">
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 3h8l4 4v14H6z M14 3v5h5" /></svg>
      <span>No file open</span>
    </div>
  </header>
</template>

<style scoped>
.file-info-bar { box-sizing: border-box; min-width: 0; min-height: 42px; padding: 0 10px; color: var(--muted); background: var(--surface); border-bottom: 1px solid var(--border); font-size: 10px; }
.summary { min-width: 0; height: 41px; display: flex; align-items: center; gap: 7px; }
.summary > svg { flex: 0 0 auto; width: 15px; height: 15px; color: var(--address); fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }
strong { max-width: min(32vw, 320px); overflow: hidden; color: var(--text); text-overflow: ellipsis; white-space: nowrap; }
.separator { color: var(--border-strong); }
.size { flex: 0 0 auto; }
.path { min-width: 48px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.modified { flex: 0 0 auto; display: inline-flex; align-items: center; gap: 5px; color: var(--modified); }
.modified i { width: 6px; height: 6px; background: currentColor; border-radius: 50%; }
button { flex: 0 0 auto; width: 26px; height: 26px; display: grid; place-items: center; margin-left: auto; padding: 0; color: var(--muted); background: transparent; border: 0; border-radius: 4px; }
button:hover { color: var(--text); background: var(--hover); }
button svg { width: 14px; height: 14px; fill: none; stroke: currentColor; stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round; }
.details { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 4px 9px; padding: 0 22px 9px; line-height: 1.45; }
.details-label { color: var(--address); font-size: 9px; letter-spacing: .08em; }
.full-path { overflow-wrap: anywhere; color: var(--text); }
.empty { color: var(--muted); }
@media (max-width: 720px) { .separator, .path { display: none; } strong { max-width: 42vw; } }
</style>
