<script setup lang="ts">
import type { ParsedField } from '../types'

defineProps<{ results: ParsedField[]; collapsed: boolean }>()
const emit = defineEmits<{ 'toggle-collapse': []; navigate: [range: { start: bigint; end: bigint }] }>()

function navigate(field: ParsedField): void {
  const start = BigInt(field.offset)
  emit('navigate', { start, end: start + BigInt(field.length) - 1n })
}
</script>

<template>
  <aside class="results-panel" :class="{ collapsed }">
    <button type="button" class="collapse" data-action="collapse-right" :title="collapsed ? 'Expand results' : 'Collapse results'" @click="emit('toggle-collapse')">{{ collapsed ? '‹' : '›' }}</button>
    <div v-if="!collapsed" class="results-content">
      <div class="panel-heading">PARSED RESULTS</div>
      <div class="table-scroll">
        <table>
          <thead><tr><th>Name</th><th>Offset</th><th>Type</th><th>Value</th><th>Comment</th></tr></thead>
          <tbody>
            <tr v-for="(field, index) in results" :key="`${field.offset}-${field.name}-${index}`" tabindex="0" @click="navigate(field)" @keydown.enter="navigate(field)">
              <td>{{ field.name }}</td><td>{{ field.offset }}</td><td>{{ field.type }}</td><td>{{ field.value }}</td><td>{{ field.comment }}</td>
            </tr>
            <tr v-if="results.length === 0" class="empty-row"><td colspan="5">Apply a template to inspect fields.</td></tr>
          </tbody>
        </table>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.results-panel { position: relative; min-width: 0; min-height: 0; background: var(--panel); border-left: 1px solid var(--border); }
.results-panel.collapsed { width: 28px; }
.collapse { position: absolute; z-index: 2; top: 5px; left: 4px; width: 21px; height: 21px; color: var(--muted); background: transparent; border: 0; border-radius: 3px; }
.collapse:hover { color: var(--text); background: var(--hover); }
.results-content { height: 100%; display: flex; flex-direction: column; padding-top: 8px; overflow: hidden; }
.panel-heading { padding: 0 8px 8px 32px; color: var(--muted); font-size: 10px; letter-spacing: .08em; }
.table-scroll { flex: 1; overflow: auto; }
table { width: 100%; border-collapse: collapse; font-size: 10px; }
th, td { max-width: 130px; padding: 7px 8px; overflow: hidden; border-bottom: 1px solid var(--border); text-align: left; text-overflow: ellipsis; white-space: nowrap; }
th { position: sticky; top: 0; color: var(--muted); background: var(--panel); font-weight: 500; }
tbody tr:not(.empty-row) { cursor: pointer; }
tbody tr:not(.empty-row):hover { background: var(--hover); }
.empty-row td { color: var(--muted); text-align: center; white-space: normal; }
</style>
