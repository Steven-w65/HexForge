<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useId, watch } from 'vue'
import type { ByteSelection } from '../hex/selection'
import type { ParsedNode, ParsedResult, TemplateSource } from '../types'
import { flattenResultLeaves, hasResultDiagnostics } from '../template/model'
import { templateStatus } from '../template/status'

const props = withDefaults(defineProps<{
  results: ParsedResult[]; collapsed: boolean; hasFile?: boolean; templateSource?: TemplateSource; templateName?: string
  templateApplied?: boolean; canApply?: boolean; canEditTemplate?: boolean; templateRange?: ByteSelection | null
  templateDirty?: boolean; resultsNeedRefresh?: boolean
}>(), { hasFile: false, templateSource: 'none', templateName: '', templateApplied: false, canApply: false, canEditTemplate: true, templateRange: null })
const emit = defineEmits<{
  'toggle-collapse': []; navigate: [range: { start: bigint; end: bigint }]
  action: [command: 'template-editor' | 'apply-template']
}>()

const activeName = computed(() => props.templateName.trim() || 'Untitled')
const resultCount = computed(() => flattenResultLeaves(props.results).length)
const expanded = ref(new Set<string>())
const viewport = ref<HTMLElement | null>(null)
const activePath = ref('')
const diagnosticPath = ref('')
const scrollTop = ref(0)
const viewportHeight = ref(180)
const treeId = useId()
const ROW_HEIGHT = 28
const OVERSCAN = 6
let known = new Map<string, ParsedNode['kind']>()
let resizeObserver: ResizeObserver | undefined
watch(() => props.results, (results) => {
  const next = new Set<string>()
  const paths = new Map<string, ParsedNode['kind']>()
  const visit = (nodes: ParsedNode[]): void => nodes.forEach(node => {
    paths.set(node.path, node.kind)
    if (node.kind === 'struct' || node.kind === 'array') {
      const keep = known.get(node.path) === node.kind ? expanded.value.has(node.path) : node.kind !== 'array' || node.children.length <= 32
      if (keep) next.add(node.path)
    }
    visit(node.children)
  })
  visit(results)
  known = paths
  expanded.value = next
  if (!paths.has(activePath.value)) activePath.value = results[0]?.path ?? ''
  if (!paths.has(diagnosticPath.value)) diagnosticPath.value = ''
}, { immediate: true })
const visibleResults = computed(() => {
  const rows: Array<{ node: ParsedNode; depth: number; parent: string | null; position: number; siblings: number }> = []
  const visit = (nodes: ParsedNode[], depth: number, parent: string | null): void => nodes.forEach((node, index) => {
    rows.push({ node, depth, parent, position: index + 1, siblings: nodes.length })
    if (expanded.value.has(node.path)) visit(node.children, depth + 1, node.path)
  })
  visit(props.results, 0, null)
  return rows
})
// A fixed-height row window bounds DOM work even with 10,000 decoded nodes.
// Long diagnostic text lives in a separate wrapping detail pane, not these rows.
const windowStart = computed(() => Math.max(0, Math.floor(scrollTop.value / ROW_HEIGHT) - OVERSCAN))
const windowEnd = computed(() => Math.min(visibleResults.value.length, Math.ceil((scrollTop.value + viewportHeight.value) / ROW_HEIGHT) + OVERSCAN))
const windowRows = computed(() => visibleResults.value.slice(windowStart.value, windowEnd.value))
const diagnosticNode = computed(() => {
  const find = (nodes: ParsedNode[]): ParsedNode | undefined => {
    for (const node of nodes) { if (node.path === diagnosticPath.value) return node; const child = find(node.children); if (child) return child }
  }
  return diagnosticPath.value ? find(props.results) : undefined
})
function readScroll(): void {
  const maximum = Math.max(0, visibleResults.value.length * ROW_HEIGHT - viewportHeight.value)
  scrollTop.value = Math.max(0, Math.min(viewport.value?.scrollTop ?? scrollTop.value, maximum))
  if (viewport.value && viewport.value.scrollTop > maximum) viewport.value.scrollTop = maximum
}
function measure(): void {
  if (viewport.value?.clientHeight) viewportHeight.value = Math.max(ROW_HEIGHT, viewport.value.clientHeight - ROW_HEIGHT)
  readScroll()
}
watch(viewport, element => {
  resizeObserver?.disconnect()
  if (element && typeof ResizeObserver !== 'undefined') { resizeObserver = new ResizeObserver(measure); resizeObserver.observe(element) }
  measure()
})
watch(visibleResults, () => { readScroll() })
onMounted(() => { window.addEventListener('resize', measure); measure() })
onBeforeUnmount(() => { resizeObserver?.disconnect(); window.removeEventListener('resize', measure) })

async function focusRow(path: string): Promise<void> {
  activePath.value = path
  const index = visibleResults.value.findIndex(entry => entry.node.path === path)
  if (index < 0) return
  const y = index * ROW_HEIGHT
  if (y < scrollTop.value) scrollTop.value = y
  else if (y + ROW_HEIGHT > scrollTop.value + viewportHeight.value) scrollTop.value = y + ROW_HEIGHT - viewportHeight.value
  if (viewport.value) viewport.value.scrollTop = scrollTop.value
  await nextTick()
  // Compare dataset values instead of constructing a CSS selector from field names.
  const row = [...(viewport.value?.querySelectorAll<HTMLElement>('[data-result-path]') ?? [])].find(element => element.dataset.resultPath === path)
  row?.focus({ preventScroll: true })
}
function onTreeKeydown(event: KeyboardEvent): void {
  const target = (event.target as HTMLElement)?.closest<HTMLElement>('[data-result-path]')?.dataset.resultPath ?? activePath.value
  let index = visibleResults.value.findIndex(entry => entry.node.path === target)
  if (index < 0) index = 0
  const entry = visibleResults.value[index]
  if (!entry || !['ArrowDown', 'ArrowUp', 'ArrowLeft', 'ArrowRight', 'Home', 'End', 'Enter', ' '].includes(event.key)) return
  event.preventDefault(); event.stopPropagation()
  if (event.key === 'ArrowDown') void focusRow(visibleResults.value[Math.min(index + 1, visibleResults.value.length - 1)]!.node.path)
  else if (event.key === 'ArrowUp') void focusRow(visibleResults.value[Math.max(0, index - 1)]!.node.path)
  else if (event.key === 'Home' || event.key === 'End') void focusRow(visibleResults.value[event.key === 'Home' ? 0 : visibleResults.value.length - 1]!.node.path)
  else if (event.key === 'ArrowRight' && (entry.node.kind === 'struct' || entry.node.kind === 'array')) {
    if (!expanded.value.has(entry.node.path)) { toggleOrNavigate(entry.node); void focusRow(entry.node.path) }
    else if (entry.node.children[0]) void focusRow(entry.node.children[0].path)
  } else if (event.key === 'ArrowLeft') {
    if (expanded.value.has(entry.node.path)) { toggleOrNavigate(entry.node); void focusRow(entry.node.path) }
    else if (entry.parent) void focusRow(entry.parent)
  } else if (event.key === 'Enter' || event.key === ' ') toggleOrNavigate(entry.node)
}

function toggleOrNavigate(node: ParsedNode): void {
  activePath.value = node.path
  diagnosticPath.value = node.diagnostics.length ? node.path : ''
  if (node.kind === 'struct' || node.kind === 'array') {
    const next = new Set(expanded.value)
    if (next.has(node.path)) next.delete(node.path)
    else next.add(node.path)
    expanded.value = next
    return
  }
  if (node.kind !== 'leaf' || node.diagnostics.length || node.offset === null || node.length === null) return
  const start = BigInt(node.offset)
  const length = BigInt(node.length)
  if (length > 0n) emit('navigate', { start, end: start + length - 1n })
}

function isActive(node: ParsedNode): boolean {
  if (!props.templateRange || node.kind !== 'leaf' || node.offset === null || node.length === null) return false
  const start = BigInt(node.offset)
  return props.templateRange.start === start && props.templateRange.end === start + BigInt(node.length) - 1n
}
const showResults = computed(() => props.hasFile && props.templateSource !== 'none' && props.templateApplied)
const showActions = computed(() => props.hasFile && props.templateSource === 'file' && !props.templateApplied)
const templateState = computed(() => templateStatus({ source: props.templateSource, dirty: Boolean(props.templateDirty), applied: props.templateApplied,
  needsRefresh: Boolean(props.resultsNeedRefresh), hasFile: props.hasFile, hasDiagnostics: hasResultDiagnostics(props.results) }))

const emptyMessage = computed(() => {
  if (props.templateSource === 'none') return props.hasFile
    ? 'No template loaded. Load or create template from Template menu.'
    : 'No template loaded.'
  if (props.templateSource === 'draft') return `Unsaved template draft: "${activeName.value}".`
  if (!props.hasFile) return `Template: "${activeName.value}" loaded. Open binary file to preview.`
  return `Template: "${activeName.value}" loaded, pending apply.`
})

function formatOffset(value: string | null): string {
  if (value === null) return '—'
  try { return `0x${BigInt(value).toString(16).toUpperCase().padStart(8, '0')}` } catch { return value }
}

</script>

<template>
  <aside class="results-panel" :class="{ collapsed }">
    <header class="results-header">
      <span class="panel-heading">PARSED RESULTS <span v-if="resultCount" class="result-count">{{ resultCount }}</span></span>
      <span data-testid="template-state" class="template-state" :title="templateSource !== 'none' ? `${activeName} · ${templateState}` : templateState">{{ templateSource !== 'none' ? `${activeName} · ${templateState}` : templateState }}</span>
      <button type="button" class="collapse" data-action="collapse-results" :title="collapsed ? 'Expand results' : 'Collapse results'" :aria-label="collapsed ? 'Expand parsed results' : 'Collapse parsed results'" @click="emit('toggle-collapse')">{{ collapsed ? '⌃' : '⌄' }}</button>
    </header>
    <div v-if="!collapsed" class="results-content">
      <div v-if="showResults" ref="viewport" class="result-list" data-testid="result-tree" role="tree" aria-label="Parsed template fields"
        :tabindex="windowRows.some(entry => entry.node.path === activePath) ? -1 : 0" @scroll="readScroll" @keydown="onTreeKeydown">
        <div class="result-columns" aria-hidden="true"><span>Path</span><span>Value / Diagnostic</span><span>Offset</span><span>Size</span><span>Type</span></div>
        <p v-if="results.length === 0" class="no-fields">No parsed fields.</p>
        <div aria-hidden="true" :style="{ height: `${windowStart * ROW_HEIGHT}px` }" />
        <button v-for="(entry, index) in windowRows" :key="entry.node.path" :id="`${treeId}-${windowStart + index}`" type="button" role="treeitem" class="result-row" data-testid="parsed-result" :data-result-path="entry.node.path"
          :tabindex="entry.node.path === activePath ? 0 : -1" @focus="activePath = entry.node.path"
          :class="{ active: isActive(entry.node), 'result-error': entry.node.diagnostics.length > 0 }"
          :aria-level="entry.depth + 1" :aria-expanded="entry.node.children.length ? expanded.has(entry.node.path) : undefined"
          :aria-posinset="entry.position" :aria-setsize="entry.siblings"
          :aria-current="isActive(entry.node) ? 'location' : undefined" :title="entry.node.comment || undefined" @click="toggleOrNavigate(entry.node)">
          <span class="result-cell result-name" :style="{ paddingLeft: `${entry.depth * 14}px` }">{{ entry.node.children.length ? expanded.has(entry.node.path) ? '▾ ' : '▸ ' : '  ' }}{{ entry.node.name }}</span>
          <span class="result-cell result-value" :class="{ 'diagnostic-value': entry.node.diagnostics.length }">{{ entry.node.diagnostics[0]?.message ?? [entry.node.value, entry.node.enumLabel, ...entry.node.flags].filter(Boolean).join(' · ') }}</span>
          <span class="result-cell result-offset">{{ formatOffset(entry.node.offset) }}</span>
          <span class="result-cell result-size">{{ entry.node.length ?? '—' }} B</span>
          <span class="result-cell result-type">{{ entry.node.type }}{{ entry.node.endianness ? ` · ${entry.node.endianness === 'little' ? 'LE' : 'BE'}` : '' }}</span>
        </button>
        <div aria-hidden="true" :style="{ height: `${(visibleResults.length - windowEnd) * ROW_HEIGHT}px` }" />
      </div>
      <div v-if="showResults && diagnosticNode?.diagnostics.length" data-testid="diagnostic-details" class="diagnostic-details" role="note">
        <strong>{{ diagnosticNode.path }}</strong>
        <p v-for="(issue, index) in diagnosticNode.diagnostics" :key="index">{{ issue.message }}</p>
      </div>
      <div v-if="!showResults" data-testid="results-empty" class="results-empty">
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
.result-columns { height: 28px; position: sticky; z-index: 1; top: 0; color: var(--muted); background: var(--surface); border-bottom: 1px solid var(--border); font-size: var(--font-support); }
.result-row { height: 28px; color: var(--text); background: transparent; border: 0; border-bottom: 1px solid var(--border); border-left: 2px solid transparent; font: inherit; text-align: left; }
.result-row:hover { background: var(--hover); }
.result-row:focus-visible { outline: 1px solid var(--selection); outline-offset: -1px; }
.result-row.active { background: color-mix(in srgb, var(--selection) 16%, var(--panel)); border-left-color: var(--selection); }
.result-row.result-error { border-left-color: var(--modified); }
.diagnostic-value { color: var(--modified); }
.diagnostic-details { flex: none; max-height: 110px; overflow: auto; padding: 8px 12px; border-top: 1px solid var(--border); color: var(--modified); font-size: var(--font-support); overflow-wrap: anywhere; }
.diagnostic-details p { margin: 5px 0 0; line-height: 1.5; white-space: pre-wrap; }
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
