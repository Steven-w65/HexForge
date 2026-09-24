<script setup lang="ts">
import { computed } from 'vue'
import type { ParsedField } from '../types'

const props = withDefaults(defineProps<{
  results: ParsedField[]; collapsed: boolean; hasFile?: boolean; templateHasFields?: boolean
}>(), { hasFile: false, templateHasFields: false })
const emit = defineEmits<{
  'toggle-collapse': []; navigate: [range: { start: bigint; end: bigint }]
  'empty-action': [command: 'open' | 'template-editor' | 'apply-template']
}>()

const emptyState = computed(() => {
  if (!props.hasFile) return { message: 'Open a binary file to inspect parsed fields.', action: 'Open File', command: 'open' as const }
  if (!props.templateHasFields) return { message: 'Create or load a parsing template.', action: 'Template Editor', command: 'template-editor' as const }
  return { message: 'The template is ready to apply.', action: 'Apply Template', command: 'apply-template' as const }
})

function navigate(field: ParsedField): void {
  const start = BigInt(field.offset)
  emit('navigate', { start, end: start + BigInt(field.length) - 1n })
}

function formatOffset(value: string): string {
  try { return `0x${BigInt(value).toString(16).toUpperCase().padStart(8, '0')}` } catch { return value }
}
</script>

<template>
  <aside class="results-panel" :class="{ collapsed }">
    <button type="button" class="collapse" data-action="collapse-right" :title="collapsed ? 'Expand results' : 'Collapse results'" :aria-label="collapsed ? 'Expand parsed results' : 'Collapse parsed results'" @click="emit('toggle-collapse')">{{ collapsed ? '‹' : '›' }}</button>
    <div v-if="!collapsed" class="results-content">
      <div class="panel-heading">PARSED RESULTS</div>
      <div v-if="results.length" class="result-list">
        <button v-for="(field, index) in results" :key="`${field.offset}-${field.name}-${index}`" type="button" data-testid="parsed-result" class="result-card" @click="navigate(field)">
          <span class="result-heading"><strong class="result-name">{{ field.name }}</strong><strong class="result-value">{{ field.value }}</strong></span>
          <span class="result-meta">{{ formatOffset(field.offset) }} · {{ field.type }} · {{ field.endianness === 'little' ? 'LE' : 'BE' }}</span>
          <span v-if="field.comment" class="result-comment">{{ field.comment }}</span>
        </button>
      </div>
      <div v-else data-testid="results-empty" class="results-empty">
        <span class="empty-icon">⌁</span>
        <p>{{ emptyState.message }}</p>
        <button type="button" data-action="results-empty-action" @click="emit('empty-action', emptyState.command)">{{ emptyState.action }}</button>
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
.panel-heading { padding: 0 8px 8px 32px; color: var(--muted); font-size: var(--font-support); letter-spacing: .08em; }
.result-list { flex: 1; min-height: 0; padding: 0 7px 8px; overflow: auto; }
.result-card { width: 100%; display: grid; gap: 4px; margin-bottom: 5px; padding: 8px; color: var(--text); background: transparent; border: 1px solid transparent; border-radius: 5px; font: inherit; text-align: left; }
.result-card:hover, .result-card:focus-visible { background: var(--hover); border-color: var(--border); outline: none; }
.result-heading { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; min-width: 0; }
.result-name, .result-value { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.result-name { font-size: var(--font-body); }
.result-value { max-width: 58%; color: var(--text); font-size: var(--font-body); font-weight: 500; text-align: right; }
.result-meta { color: var(--address); font-size: var(--font-support); }
.result-comment { overflow: hidden; color: var(--muted); font-size: var(--font-support); text-overflow: ellipsis; white-space: nowrap; }
.results-empty { flex: 1; display: grid; place-content: center; justify-items: center; gap: 7px; padding: 18px; color: var(--muted); text-align: center; }
.results-empty p { max-width: 190px; margin: 0; font-size: var(--font-body); line-height: 1.5; }
.empty-icon { font-size: 20px; color: var(--address); }
.results-empty button { height: 27px; padding: 0 10px; color: var(--text); background: var(--button); border: 0; border-radius: 4px; font: inherit; font-size: var(--font-body); }
.results-empty button:hover { background: var(--hover); }
</style>
