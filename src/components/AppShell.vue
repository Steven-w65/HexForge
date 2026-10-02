<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { BytesPerRow } from '../hex/layout'
import { DEFAULT_MINIMAP_SETTINGS, type MinimapSettings } from '../hex/minimapGeometry'
import type { ByteSelection } from '../hex/selection'
import type { Endian, FileInfo, ModifiedOverview, PageRequest, ParsedResult, TemplateDefinition, TemplateSource, ViewportPage } from '../types'
import type { ColorTheme } from '../types'
import { flattenResultLeaves, hasResultDiagnostics } from '../template/model'
import { commandEnabled, commandUnavailableReason, type MenuCommand, type MenuState } from '../menu/commands'
import AppDialog from './AppDialog.vue'
import FileInfoBar from './FileInfoBar.vue'
import HexCanvas from './HexCanvas.vue'
import ParsedResultsPanel from './ParsedResultsPanel.vue'
import StatusBar from './StatusBar.vue'
import TopMenu from './TopMenu.vue'
import ResultsSplitter from './ResultsSplitter.vue'

const props = withDefaults(defineProps<{
  file?: FileInfo | null; sourceIdentity?: number; page?: ViewportPage | null; bytesPerRow?: BytesPerRow; selection?: ByteSelection | null
  matches?: bigint[]; modifiedOverview?: ModifiedOverview; templateRange?: ByteSelection | null; editMode?: boolean; endianness?: Endian
  template?: TemplateDefinition; results?: ParsedResult[]; dialogOpen?: boolean; dialogTitle?: string; dialogMessage?: string
  navigationOffset?: bigint
  matchLength?: number; busyLabel?: string; progressText?: string; searchTruncated?: boolean
  searchOpen?: boolean; searchValue?: string; searchQuery?: string; searchCount?: number; searchBusy?: boolean; searchError?: string; searchFocusKey?: number
  searchIndex?: number; searchCanNavigate?: boolean; searchNeedsUpdate?: boolean
  gotoOpen?: boolean; gotoValue?: string; gotoError?: string; gotoFocusKey?: number
  templateValid?: boolean
  templateSource?: TemplateSource; templateDisplayName?: string; templateApplied?: boolean
  templateDirty?: boolean; resultsNeedRefresh?: boolean; completionNotice?: { id: number; text: string } | null
  menuState?: MenuState; theme?: ColorTheme; rightCollapsed?: boolean
  minimapSettings?: MinimapSettings; editDelta?: { offset: bigint; revision: string } | null
}>(), {
  file: null, sourceIdentity: 0, page: null, bytesPerRow: 16, selection: null, matches: () => [], modifiedOverview: () => ({ binCount: 1024, bins: [] }), templateRange: null,
  editMode: false, endianness: 'little', template: () => ({ name: 'Untitled', defaultEndianness: 'little', fields: [] }), results: () => [], dialogOpen: false,
  dialogTitle: '', dialogMessage: '',
  matchLength: 1, busyLabel: '', progressText: '', searchTruncated: false,
  searchOpen: false, searchValue: '', searchQuery: '', searchCount: 0, searchBusy: false, searchError: '', searchFocusKey: 0,
  searchIndex: -1, searchCanNavigate: false, searchNeedsUpdate: false,
  gotoOpen: false, gotoValue: '', gotoError: '', gotoFocusKey: 0,
  templateValid: true, templateSource: 'none', templateDisplayName: '', templateApplied: false,
  templateDirty: false, resultsNeedRefresh: false, completionNotice: null,
  menuState: () => ({ hasFile: false, hasBytes: false, singleByteSelected: false, editMode: false, canUndo: false, templateValid: true, templateActive: false, templateHasPath: false, templateHasFields: false, hasParsedResults: false, operationBusy: false }),
  theme: 'dark', rightCollapsed: false,
  minimapSettings: () => ({ ...DEFAULT_MINIMAP_SETTINGS }), editDelta: null,
})

const emit = defineEmits<{
  command: [value: MenuCommand]
  'update:bytesPerRow': [value: BytesPerRow]; navigate: [range: { start: bigint; end: bigint }]
  'request-page': [request: PageRequest]; select: [selection: ByteSelection]; 'edit-request': [offset: bigint]
  'viewport-offset': [offset: bigint]; 'close-dialog': []
  'minimap-error': [error: unknown]
  'update:searchValue': [value: string]; 'submit-search': [force?: boolean, direction?: 1 | -1]; 'navigate-search': [direction: 1 | -1]; 'clear-search': []; 'close-search': []
  'update:gotoValue': [value: string]; 'submit-goto': []; 'close-goto': []
}>()

const searchInput = ref<HTMLInputElement | null>(null)
const workspace = ref<HTMLElement | null>(null)
const PANEL_HEIGHT_KEY = 'hexforge.resultsPaneHeight'
function readPanelHeight(): number {
  try { const value = Number(localStorage.getItem(PANEL_HEIGHT_KEY)); return Number.isFinite(value) && value >= 28 ? value : 220 }
  catch { return 220 }
}
const preferredPanelHeight = ref(readPanelHeight())
const workspaceHeight = ref(Math.max(28, window.innerHeight - 90))
const panelMaximum = computed(() => Math.max(28, Math.floor(workspaceHeight.value - 164)))
const panelMinimum = computed(() => Math.min(84, panelMaximum.value))
const panelHeight = computed(() => Math.min(panelMaximum.value, Math.max(panelMinimum.value, preferredPanelHeight.value)))
let workspaceObserver: ResizeObserver | undefined
function measureWorkspace(): void {
  const height = workspace.value?.getBoundingClientRect().height
  workspaceHeight.value = height && height > 0 ? height : Math.max(28, window.innerHeight - 90)
}
function storePanelHeight(): void {
  try { localStorage.setItem(PANEL_HEIGHT_KEY, String(panelHeight.value)) } catch { /* Optional preference, never required for resizing. */ }
}
onMounted(() => {
  measureWorkspace(); window.addEventListener('resize', measureWorkspace)
  if (typeof ResizeObserver !== 'undefined' && workspace.value) { workspaceObserver = new ResizeObserver(measureWorkspace); workspaceObserver.observe(workspace.value) }
})
onBeforeUnmount(() => { workspaceObserver?.disconnect(); window.removeEventListener('resize', measureWorkspace) })
const gotoInput = ref<HTMLInputElement | null>(null)
function onSearchEnter(event: KeyboardEvent): void {
  if (event.isComposing || event.ctrlKey || event.altKey || event.metaKey) return
  event.preventDefault()
  emit('submit-search', false, event.shiftKey ? -1 : 1)
}
watch([() => props.searchOpen, () => props.searchFocusKey], ([open]) => {
  if (open) void nextTick(() => searchInput.value?.focus())
})
watch([() => props.gotoOpen, () => props.gotoFocusKey], ([open]) => {
  if (open) void nextTick(() => gotoInput.value?.focus())
})

const fileSize = computed(() => BigInt(props.file?.size ?? '0'))
const parsedLeaves = computed(() => flattenResultLeaves(props.results))
const minimapFields = computed(() => {
  const range = props.templateRange
  if (!range) return []
  // Share the active result's exact byte range with the canvas and overview ruler,
  // rather than marking every decoded field just because a template was applied.
  return parsedLeaves.value.filter(field => {
    const start = BigInt(field.offset)
    return start === range.start && start + BigInt(field.length) - 1n === range.end
  })
})
const effectiveMenuState = computed<MenuState>(() => ({
  ...props.menuState,
  templateValid: props.templateValid,
  templateHasFields: props.template.fields.length > 0,
  hasParsedResults: parsedLeaves.value.length > 0 && !hasResultDiagnostics(props.results),
}))
const selectedByte = computed(() => {
  if (!props.selection) return null
  const offset = props.selection.start
  if (!props.page) return { offset, value: null }
  const index = offset - BigInt(props.page.offset)
  if (index < 0n || index >= BigInt(props.page.bytes.length)) return { offset, value: null }
  return { offset, value: props.page.bytes[Number(index)]! }
})

</script>

<template>
  <main
    data-testid="app-shell"
    class="app-shell"
    :class="{ 'results-collapsed': rightCollapsed }"
    :style="{ '--results-pane-height': `${panelHeight}px`, '--top-menu-height': '34px', '--minimap-width': minimapSettings.enabled ? '96px' : '12px' }"
    role="application"
    @contextmenu.prevent
  >
    <TopMenu :state="effectiveMenuState" :bytes-per-row="bytesPerRow"
      :results="results" :minimap-settings="minimapSettings" @command="emit('command', $event)" @navigate="emit('navigate', $event)" />
    <FileInfoBar :file="file" />
    <div ref="workspace" class="workspace">
      <section class="hex-stage">
        <HexCanvas v-if="file" :file-size="fileSize" :source-identity="sourceIdentity" :source-key="file.path" :source-revision="file.revision" :page="page" :bytes-per-row="bytesPerRow" :selection="selection" :matches="matches" :modified-overview="modifiedOverview" :template-fields="minimapFields" :match-length="matchLength"
          :template-range="templateRange" :edit-mode="editMode" :theme="theme" :navigate-offset="navigationOffset" :minimap-settings="minimapSettings" :edit-delta="editDelta" @request-page="emit('request-page', $event)"
          @select="emit('select', $event)" @edit-request="emit('edit-request', $event)" @viewport-offset="emit('viewport-offset', $event)" @minimap-error="emit('minimap-error', $event)" />
        <form v-if="searchOpen && file" data-testid="search-bar" class="floating-bar" role="search" @submit.prevent="emit('submit-search')">
          <div class="floating-bar__controls">
            <label class="visually-hidden" for="search-bytes">Search hex bytes</label>
            <input id="search-bytes" ref="searchInput" data-testid="search-input" :value="searchValue" placeholder="41 42 43" spellcheck="false" autocomplete="off"
              @input="emit('update:searchValue', ($event.target as HTMLInputElement).value)" @keydown.enter="onSearchEnter">
            <button type="button" data-action="run-search" :disabled="searchBusy || effectiveMenuState.operationBusy" @click="emit('submit-search', true)">Search</button>
            <button type="button" class="floating-bar__arrow" data-action="previous-search-match" aria-label="Previous search match" title="Previous match (Shift+F3)" :disabled="!searchCanNavigate" @click="emit('navigate-search', -1)">↑</button>
            <button type="button" class="floating-bar__arrow" data-action="next-search-match" aria-label="Next search match" title="Next match (F3)" :disabled="!searchCanNavigate" @click="emit('navigate-search', 1)">↓</button>
            <button type="button" data-action="clear-search" :disabled="searchBusy || (!searchValue && !searchQuery)" @click="emit('clear-search')">Clear</button>
            <button type="button" class="floating-bar__close" data-action="close-search" aria-label="Close search" @click="emit('close-search')">×</button>
          </div>
          <div v-if="searchError" class="floating-bar__detail floating-bar__detail--error" role="alert">{{ searchError }}</div>
          <div v-else class="floating-bar__detail" data-testid="search-count" role="status">
            {{ searchBusy ? 'Searching…' : searchQuery ? `${searchIndex >= 0 ? `${searchIndex + 1} of ${searchCount}` : `${searchCount} ${searchCount === 1 ? 'match' : 'matches'}`}${searchTruncated ? ' (limited)' : ''}` : 'Enter hexadecimal bytes, then press Enter.' }}
            <template v-if="searchNeedsUpdate && !searchBusy"> · Search to update results.</template>
          </div>
        </form>
        <form v-if="gotoOpen && file" data-testid="goto-bar" class="floating-bar" aria-label="Go to offset" @submit.prevent="emit('submit-goto')">
          <div class="floating-bar__controls">
            <label class="visually-hidden" for="goto-offset">Go to offset</label>
            <input id="goto-offset" ref="gotoInput" data-testid="goto-input" :value="gotoValue" placeholder="0x100 or 256" spellcheck="false" autocomplete="off"
              @input="emit('update:gotoValue', ($event.target as HTMLInputElement).value)">
            <button type="submit">Go</button>
            <button type="button" class="floating-bar__close" data-action="close-goto" aria-label="Close Go To" @click="emit('close-goto')">×</button>
          </div>
          <div v-if="gotoError" data-testid="goto-error" class="floating-bar__detail floating-bar__detail--error" role="alert">{{ gotoError }}</div>
          <div v-else class="floating-bar__detail">Enter a decimal or 0x-prefixed hexadecimal offset.</div>
        </form>
        <div v-if="!file" data-testid="drop-prompt" class="drop-prompt">
          <span>＋</span>
          <p>Drop a binary file</p>
          <button type="button" data-action="empty-open" :disabled="!commandEnabled('open', effectiveMenuState)"
            :title="commandUnavailableReason('open', effectiveMenuState) ?? 'Open File (Ctrl+O)'" @click="emit('command', 'open')">Open File</button>
        </div>
        <div v-if="file && fileSize === 0n" data-testid="empty-file" class="empty-file">This binary file is empty.</div>
        <div v-if="searchTruncated && !searchOpen" data-testid="search-truncated" class="search-notice" role="status">Search results were limited; refine the byte pattern.</div>
      </section>
      <ResultsSplitter v-if="!rightCollapsed" :model-value="panelHeight" :minimum="panelMinimum" :maximum="panelMaximum" @update:model-value="preferredPanelHeight = $event" @commit="storePanelHeight" />
      <ParsedResultsPanel :results="results" :collapsed="rightCollapsed" :has-file="Boolean(file)" :template-source="templateSource"
        :template-dirty="templateDirty" :results-need-refresh="resultsNeedRefresh"
        :template-name="templateDisplayName || template.name" :template-applied="templateApplied" :template-range="templateRange"
        :can-apply="commandEnabled('apply-template', effectiveMenuState)" :can-edit-template="commandEnabled('template-editor', effectiveMenuState)"
        @toggle-collapse="emit('command', 'toggle-right-panel')" @navigate="emit('navigate', $event)" @action="emit('command', $event)" />
    </div>
    <StatusBar :file-size="fileSize" :selected="selectedByte" :selected-count="selection?.count ?? 0n" :bytes-per-row="bytesPerRow"
      :edit-mode="editMode" :endianness="endianness" :dirty="file?.dirty ?? false" :busy-label="busyLabel" :progress-text="progressText" :completion-notice="completionNotice"
      @update:bytes-per-row="emit('update:bytesPerRow', $event)" />
    <AppDialog :open="dialogOpen" :title="dialogTitle" :message="dialogMessage" @close="emit('close-dialog')"><slot name="dialog" /></AppDialog>
  </main>
</template>

<style scoped>
.app-shell { position: relative; width: 100vw; height: 100vh; display: grid; grid-template-rows: var(--top-menu-height) auto minmax(0, 1fr) 24px; color: var(--text); background: var(--bg); overflow: hidden; }
.workspace { position: relative; display: grid; grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, 1fr) 4px var(--results-pane-height); min-width: 0; min-height: 0; }
.app-shell.results-collapsed .workspace { grid-template-rows: minmax(0, 1fr) 28px; }
.hex-stage { position: relative; min-width: 0; min-height: 0; overflow: hidden; }
.search-notice { position: absolute; z-index: 2; top: 10px; left: 12px; padding: 5px 8px; color: var(--modified); background: color-mix(in srgb, var(--surface) 92%, transparent); border: 1px solid var(--border); border-radius: 4px; font-size: var(--font-support); pointer-events: none; }
.drop-prompt { position: absolute; inset: 0; display: grid; place-content: center; justify-items: center; gap: 7px; color: var(--muted); }
.drop-prompt span { display: grid; place-items: center; width: 42px; height: 42px; color: var(--address); border: 1px dashed var(--border-strong); border-radius: 8px; font-size: 22px; }
.drop-prompt p { margin: 0; font-size: var(--font-body); text-align: center; }
.drop-prompt button { height: 28px; margin-top: 4px; padding: 0 12px; color: var(--text); background: var(--button); border: 0; border-radius: 3px; font: inherit; font-size: var(--font-body); cursor: pointer; }
.drop-prompt button:hover:not(:disabled) { background: var(--hover); }
.drop-prompt button:focus-visible { outline: 1px solid var(--selection); outline-offset: 2px; }
.drop-prompt button:disabled { opacity: .4; cursor: default; }
.empty-file { position: absolute; inset: 0 var(--minimap-width) 0 0; display: grid; place-items: center; color: var(--muted); background: var(--bg); font-size: var(--font-body); pointer-events: none; }
.floating-bar { position: absolute; z-index: 3; top: 7px; right: calc(var(--minimap-width, 40px) + 10px); width: min(390px, calc(100% - var(--minimap-width, 40px) - 20px)); box-sizing: border-box; padding: 7px; background: var(--panel); border: 1px solid var(--border-strong); border-radius: 5px; box-shadow: 0 5px 14px rgb(0 0 0 / 15%); }
.floating-bar__controls { display: flex; align-items: center; gap: 5px; min-width: 0; }
.floating-bar input { flex: 1; min-width: 50px; height: 27px; padding: 0 7px; color: var(--text); background: var(--input); border: 1px solid var(--border); border-radius: 3px; font: inherit; font-size: var(--font-body); outline: none; }
.floating-bar input:focus { border-color: var(--selection); }
.floating-bar button { height: 27px; padding: 0 7px; color: var(--text); background: var(--button); border: 0; border-radius: 3px; font: inherit; font-size: var(--font-support); white-space: nowrap; }
.floating-bar button:hover:not(:disabled) { background: var(--hover); }
.floating-bar button:disabled { opacity: .4; }
.floating-bar .floating-bar__arrow { width: 24px; padding: 0; }
.floating-bar .floating-bar__close { width: 26px; padding: 0; color: var(--muted); background: transparent; font-size: 19px; }
.floating-bar__detail { min-height: 15px; padding: 5px 2px 0; color: var(--muted); font-size: var(--font-support); }
.floating-bar__detail--error { color: var(--modified); }
.visually-hidden { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
</style>
