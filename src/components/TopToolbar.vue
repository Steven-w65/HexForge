<script setup lang="ts">
import type { ColorTheme } from '../types'

defineProps<{ hasFile: boolean; dirty: boolean; editMode: boolean; theme: ColorTheme }>()
const emit = defineEmits<{
  open: []; goto: []; search: []; template: []; export: []; theme: []; edit: []; 'save-as': []
}>()

const actions = [
  { id: 'open', label: 'Open File', path: 'M4 6h5l2 2h9v10H4z M4 10h16' },
  { id: 'goto', label: 'Go To Offset', path: 'M5 12h13 M14 8l4 4-4 4' },
  { id: 'search', label: 'Search Bytes', path: 'M10 17a7 7 0 1 1 0-14 7 7 0 0 1 0 14z M15 15l5 5' },
  { id: 'template', label: 'Apply Template', path: 'M5 3h10l4 4v14H5z M15 3v5h5 M8 12h8 M8 16h8' },
  { id: 'export', label: 'Export', path: 'M12 3v12 M8 7l4-4 4 4 M5 14v7h14v-7' },
  { id: 'theme', label: 'Theme Toggle', path: 'M12 3a9 9 0 1 0 9 9c-4 2-9-1-9-9z' },
  { id: 'edit', label: 'Edit Mode Toggle', path: 'M4 20l4-1 10-10-3-3L5 16z M13 8l3 3' },
  { id: 'save-as', label: 'Save As', path: 'M5 3h12l3 3v15H5z M8 3v6h8V3 M8 17h8' },
] as const

function invoke(id: typeof actions[number]['id']): void {
  switch (id) {
    case 'open': emit('open'); break
    case 'goto': emit('goto'); break
    case 'search': emit('search'); break
    case 'template': emit('template'); break
    case 'export': emit('export'); break
    case 'theme': emit('theme'); break
    case 'edit': emit('edit'); break
    case 'save-as': emit('save-as'); break
  }
}
</script>

<template>
  <header class="toolbar">
    <div class="brand"><span class="brand-mark">HF</span><strong>HexForge</strong><span v-if="dirty" class="dirty-dot" title="Modified" /></div>
    <nav aria-label="File and analysis tools">
      <button
        v-for="action in actions"
        :key="action.id"
        type="button"
        :data-action="action.id"
        :title="action.label"
        :aria-label="action.label"
        :aria-pressed="action.id === 'edit' ? editMode : action.id === 'theme' ? theme === 'light' : undefined"
        :disabled="action.id !== 'open' && action.id !== 'theme' && !hasFile"
        @click="invoke(action.id)"
      >
        <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="action.path" /></svg>
        <span>{{ action.label }}</span>
      </button>
    </nav>
  </header>
</template>

<style scoped>
.toolbar { height: 42px; display: flex; align-items: center; gap: 24px; padding: 0 10px; background: var(--surface); border-bottom: 1px solid var(--border); }
.brand { display: flex; align-items: center; gap: 8px; white-space: nowrap; letter-spacing: .02em; }
.brand-mark { display: grid; place-items: center; width: 24px; height: 24px; color: var(--selection-text); background: var(--selection); border-radius: 5px; font-size: 10px; font-weight: 700; }
.dirty-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--modified); }
nav { display: flex; min-width: 0; height: 100%; align-items: center; overflow: hidden; }
button { height: 32px; display: inline-flex; align-items: center; gap: 6px; padding: 0 9px; color: var(--muted); background: transparent; border: 0; border-radius: 4px; font: inherit; font-size: 11px; white-space: nowrap; }
button:hover:not(:disabled), button[aria-pressed='true'] { color: var(--text); background: var(--hover); }
button:disabled { opacity: .36; }
svg { width: 15px; height: 15px; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }
@media (max-width: 920px) { button span { display: none; } button { padding-inline: 8px; } }
</style>
