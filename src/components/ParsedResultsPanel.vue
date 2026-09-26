<script setup lang="ts">
import { computed } from 'vue'
import type { ByteSelection } from '../hex/selection'
import type { ParsedField, TemplateSource } from '../types'

const props = withDefaults(defineProps<{
  results: ParsedField[]; collapsed: boolean; hasFile?: boolean; templateSource?: TemplateSource; templateName?: string
  templateApplied?: boolean; canApply?: boolean; canEditTemplate?: boolean; templateRange?: ByteSelection | null
}>(), { hasFile: false, templateSource: 'none', templateName: '', templateApplied: false, canApply: false, canEditTemplate: true, templateRange: null })
const emit = defineEmits<{
  'toggle-collapse': []; navigate: [range: { start: bigint; end: bigint }]
  action: [command: 'template-editor' | 'apply-template']
}>()

const activeName = computed(() => props.templateName.trim() || 'Untitled')
const showResults = computed(() => props.hasFile && props.templateSource !== 'none' && props.templateApplied)
const showActions = computed(() => props.hasFile && props.templateSource === 'file' && !props.templateApplied)
const templateState = computed(() => {
  if (props.templateSource === 'none') return 'No template'
  if (props.templateSource === 'draft') return showResults.value ? 'Applied draft' : 'Unsaved draft'
  if (!props.hasFile) return 'Loaded'
  return props.templateApplied ? 'Applied' : 'Pending apply'
})

const emptyMessage = computed(() => {
  if (props.templateSource === 'none') return props.hasFile
    ? 'No template loaded. Load or create template from Template menu.'
    : 'No template loaded.'
  if (props.templateSource === 'draft') return `Unsaved template draft: "${activeName.value}".`
  if (!props.hasFile) return `Template: "${activeName.value}" loaded. Open binary file to preview.`
  return `Template: "${activeName.value}" loaded, pending apply.`
})

function navigate(field: ParsedField): void {
  const start = BigInt(field.offset)
  emit('navigate', { start, end: start + BigInt(field.length) - 1n })
}

function formatOffset(value: string): string {
  try { return `0x${BigInt(value).toString(16).toUpperCase().padStart(8, '0')}` } catch { return value }
}

function isActive(field: ParsedField): boolean {
  if (!props.templateRange) return false
  const start = BigInt(field.offset)
  return props.templateRange.start === start && props.templateRange.end === start + BigInt(field.length) - 1n
}
</script>

<template>
  <aside class="results-panel" :class="{ collapsed }">
    <header class="results-header">
      <span class="panel-heading">PARSED RESULTS <span v-if="results.length" class="result-count">{{ results.length }}</span></span>
      <span data-testid="template-state" class="template-state" :title="templateSource !== 'none' ? `${activeName} · ${templateState}` : templateState">{{ templateSource !== 'none' ? `${activeName} · ${templateState}` : templateState }}</span>
      <button type="button" class="collapse" data-action="collapse-results" :title="collapsed ? 'Expand results' : 'Collapse results'" :aria-label="collapsed ? 'Expand parsed results' : 'Collapse parsed results'" @click="emit('toggle-collapse')">{{ collapsed ? '⌃' : '⌄' }}</button>
    </header>
    <div v-if="!collapsed" class="results-content">
      <div v-if="showResults" class="result-list">
        <div class="result-columns" aria-hidden="true"><span>Name</span><span>Value</span><span>Offset</span><span>Size</span><span>Type</span></div>
        <p v-if="results.length === 0" class="no-fields">No parsed fields.</p>
        <button v-for="(field, index) in results" :key="`${field.offset}-${field.name}-${index}`" type="button" data-testid="parsed-result" class="result-row" :class="{ active: isActive(field) }"
          :aria-current="isActive(field) ? 'location' : undefined" :title="field.comment || undefined" @click="navigate(field)">
          <span class="result-cell result-name">{{ field.name }}</span>
          <span class="result-cell result-value">{{ field.value }}</span>
          <span class="result-cell result-offset">{{ formatOffset(field.offset) }}</span>
          <span class="result-cell result-size">{{ field.length }} B</span>
          <span class="result-cell result-type">{{ field.type }} · {{ field.endianness === 'little' ? 'LE' : 'BE' }}</span>
        </button>
      </div>
      <div v-else data-testid="results-empty" class="results-empty">
        <p>{{ emptyMessage }}</p>
        <div v-if="showActions" class="results-actions">
          <button type="button" data-action="results-template-editor" :disabled="!canEditTemplate" @click="emit('action', 'template-editor')">Template Editor</button>
          <button type="button" data-action="results-apply-template" :disabled="!canApply" @click="emit('action', 'apply-template')">Apply Template</button>
        </div>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.results-panel { min-width: 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden; background: var(--panel); border-top: 1px solid var(--border); }
.results-header { box-sizing: border-box; height: 28px; flex: none; display: flex; align-items: center; gap: 10px; padding: 0 8px 0 12px; border-bottom: 1px solid var(--border); }
.results-panel.collapsed .results-header { border-bottom: 0; }
.collapse { width: 22px; height: 22px; color: var(--muted); background: transparent; border: 0; border-radius: 3px; font-size: 16px; line-height: 1; }
.collapse:hover { color: var(--text); background: var(--hover); }
.results-content { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }
.panel-heading { color: var(--muted); font-size: var(--font-support); letter-spacing: .08em; }
.template-state { min-width: 0; margin-left: auto; overflow: hidden; color: var(--address); font-size: var(--font-support); text-overflow: ellipsis; white-space: nowrap; }
.result-count { margin-left: 5px; color: var(--address); letter-spacing: 0; }
.result-list { flex: 1; min-height: 0; overflow: auto; }
.result-columns, .result-row { box-sizing: border-box; width: 100%; min-width: 620px; display: grid; grid-template-columns: minmax(120px, 1.2fr) minmax(160px, 1.7fr) 130px 70px 100px; align-items: center; gap: 8px; padding: 4px 10px; }
.result-columns { position: sticky; z-index: 1; top: 0; color: var(--muted); background: var(--surface); border-bottom: 1px solid var(--border); font-size: var(--font-support); }
.result-row { min-height: 28px; color: var(--text); background: transparent; border: 0; border-bottom: 1px solid var(--border); border-left: 2px solid transparent; font: inherit; text-align: left; }
.result-row:hover { background: var(--hover); }
.result-row:focus-visible { outline: 1px solid var(--selection); outline-offset: -1px; }
.result-row.active { background: color-mix(in srgb, var(--selection) 16%, var(--panel)); border-left-color: var(--selection); }
.result-cell { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.result-name { font-weight: 600; }
.result-offset, .result-size, .result-type { color: var(--address); }
.no-fields { margin: 16px; color: var(--muted); font-size: var(--font-body); }
.results-empty { flex: 1; display: grid; place-content: center; justify-items: center; gap: 7px; padding: 18px; color: var(--muted); text-align: center; }
.results-empty p { margin: 0; font-size: var(--font-body); line-height: 1.5; }
.results-actions { display: flex; justify-content: center; gap: 8px; margin-top: 4px; }
.results-empty button { height: 27px; padding: 0 10px; color: var(--text); background: var(--button); border: 0; border-radius: 4px; font: inherit; font-size: var(--font-body); }
.results-empty button:hover { background: var(--hover); }
.results-empty button:disabled { opacity: .38; cursor: default; }
</style>
