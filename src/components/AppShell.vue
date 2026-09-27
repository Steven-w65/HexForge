<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import type { BytesPerRow } from '../hex/layout'
import type { ByteSelection } from '../hex/selection'
import type { Endian, FileInfo, ModifiedOverview, PageRequest, ParsedField, TemplateDefinition, TemplateSource, ViewportPage } from '../types'
import type { ColorTheme } from '../types'
import { commandEnabled, type MenuCommand, type MenuState } from '../menu/commands'
import AppDialog from './AppDialog.vue'
import FileInfoBar from './FileInfoBar.vue'
import HexCanvas from './HexCanvas.vue'
import ParsedResultsPanel from './ParsedResultsPanel.vue'
import StatusBar from './StatusBar.vue'
import TopMenu from './TopMenu.vue'

const props = withDefaults(defineProps<{
  file?: FileInfo | null; sourceIdentity?: number; page?: ViewportPage | null; bytesPerRow?: BytesPerRow; selection?: ByteSelection | null
  matches?: bigint[]; modifiedOverview?: ModifiedOverview; templateRange?: ByteSelection | null; editMode?: boolean; endianness?: Endian
  template?: TemplateDefinition; results?: ParsedField[]; dialogOpen?: boolean; dialogTitle?: string; dialogMessage?: string
  navigationOffset?: bigint
  matchLength?: number; busyLabel?: string; progressText?: string; searchTruncated?: boolean
  searchOpen?: boolean; searchValue?: string; searchQuery?: string; searchCount?: number; searchBusy?: boolean; searchError?: string; searchFocusKey?: number
  templateValid?: boolean
  templateSource?: TemplateSource; templateDisplayName?: string; templateApplied?: boolean
  menuState?: MenuState; theme?: ColorTheme; rightCollapsed?: boolean
}>(), {
  file: null, sourceIdentity: 0, page: null, bytesPerRow: 16, selection: null, matches: () => [], modifiedOverview: () => ({ binCount: 1024, bins: [] }), templateRange: null,
  editMode: false, endianness: 'little', template: () => ({ version: 1, name: 'Untitled', defaultEndianness: 'little', fields: [] }), results: () => [], dialogOpen: false,
  dialogTitle: '', dialogMessage: '',
  matchLength: 1, busyLabel: '', progressText: '', searchTruncated: false,
  searchOpen: false, searchValue: '', searchQuery: '', searchCount: 0, searchBusy: false, searchError: '', searchFocusKey: 0,
  templateValid: true, templateSource: 'none', templateDisplayName: '', templateApplied: false,
  menuState: () => ({ hasFile: false, hasBytes: false, singleByteSelected: false, editMode: false, canUndo: false, templateValid: true, templateActive: false, templateHasPath: false, templateHasFields: false, hasNavigableTemplateFields: false, hasParsedResults: false, operationBusy: false }),
  theme: 'dark', rightCollapsed: false,
})

const emit = defineEmits<{
  command: [value: MenuCommand]
  'update:bytesPerRow': [value: BytesPerRow]; navigate: [range: { start: bigint; end: bigint }]
  'request-page': [request: PageRequest]; select: [selection: ByteSelection]; 'edit-request': [offset: bigint]
  'viewport-offset': [offset: bigint]; 'close-dialog': []
  'update:searchValue': [value: string]; 'submit-search': []; 'clear-search': []; 'close-search': []
}>()

const searchInput = ref<HTMLInputElement | null>(null)
watch([() => props.searchOpen, () => props.searchFocusKey], ([open]) => {
  if (open) void nextTick(() => searchInput.value?.focus())
})

const fileSize = computed(() => BigInt(props.file?.size ?? '0'))
const effectiveMenuState = computed<MenuState>(() => ({
  ...props.menuState,
  templateValid: props.templateValid,
  templateHasFields: props.template.fields.length > 0,
  hasParsedResults: props.results.length > 0,
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
    :style="{ '--results-pane-height': '220px', '--top-menu-height': '34px', '--minimap-width': '96px' }"
    role="application"
    @contextmenu.prevent
  >
    <TopMenu :state="effectiveMenuState" :bytes-per-row="bytesPerRow"
      :file-size="fileSize" :template="template" :results="results" @command="emit('command', $event)" @navigate="emit('navigate', $event)" />
    <FileInfoBar :file="file" />
    <div class="workspace">
      <section class="hex-stage">
        <HexCanvas v-if="file" :file-size="fileSize" :source-identity="sourceIdentity" :source-key="file.path" :source-revision="file.revision" :page="page" :bytes-per-row="bytesPerRow" :selection="selection" :matches="matches" :modified-overview="modifiedOverview" :template-fields="results" :match-length="matchLength"
          :template-range="templateRange" :edit-mode="editMode" :theme="theme" :navigate-offset="navigationOffset" @request-page="emit('request-page', $event)"
          @select="emit('select', $event)" @edit-request="emit('edit-request', $event)" @viewport-offset="emit('viewport-offset', $event)" />
        <form v-if="searchOpen && file" data-testid="search-bar" class="search-bar" role="search" @submit.prevent="emit('submit-search')">
          <div class="search-bar__controls">
            <label class="visually-hidden" for="search-bytes">Search hex bytes</label>
            <input id="search-bytes" ref="searchInput" data-testid="search-input" :value="searchValue" placeholder="41 42 43" spellcheck="false" autocomplete="off"
              @input="emit('update:searchValue', ($event.target as HTMLInputElement).value)">
            <button type="submit" :disabled="searchBusy">Search</button>
            <button type="button" data-action="clear-search" :disabled="searchBusy || (!searchValue && !searchQuery)" @click="emit('clear-search')">Clear</button>
            <button type="button" class="search-bar__close" data-action="close-search" aria-label="Close search" @click="emit('close-search')">×</button>
          </div>
          <div v-if="searchError" class="search-bar__detail search-bar__detail--error" role="alert">{{ searchError }}</div>
          <div v-else class="search-bar__detail" data-testid="search-count" role="status">
            {{ searchBusy ? 'Searching…' : searchQuery ? `${searchCount} ${searchCount === 1 ? 'match' : 'matches'}${searchTruncated ? ' (limited)' : ''}` : 'Enter hexadecimal bytes, then press Enter.' }}
          </div>
        </form>
        <div v-if="!file" data-testid="drop-prompt" class="drop-prompt">
          <span>＋</span><strong>Drop a binary file or use File → Open File</strong>
        </div>
        <div v-if="file && fileSize === 0n" data-testid="empty-file" class="empty-file">This binary file is empty.</div>
        <div v-if="searchTruncated && !searchOpen" data-testid="search-truncated" class="search-notice" role="status">Search results were limited; refine the byte pattern.</div>
      </section>
      <ParsedResultsPanel :results="results" :collapsed="rightCollapsed" :has-file="Boolean(file)" :template-source="templateSource"
        :template-name="templateDisplayName || template.name" :template-applied="templateApplied" :template-range="templateRange"
        :can-apply="commandEnabled('apply-template', effectiveMenuState)" :can-edit-template="commandEnabled('template-editor', effectiveMenuState)"
        @toggle-collapse="emit('command', 'toggle-right-panel')" @navigate="emit('navigate', $event)" @action="emit('command', $event)" />
    </div>
    <StatusBar :file-size="fileSize" :selected="selectedByte" :selected-count="selection?.count ?? 0n" :bytes-per-row="bytesPerRow"
      :edit-mode="editMode" :endianness="endianness" :dirty="file?.dirty ?? false" :busy-label="busyLabel" :progress-text="progressText"
      @update:bytes-per-row="emit('update:bytesPerRow', $event)" />
    <AppDialog :open="dialogOpen" :title="dialogTitle" :message="dialogMessage" @close="emit('close-dialog')"><slot name="dialog" /></AppDialog>
  </main>
</template>

<style scoped>
.app-shell { position: relative; width: 100vw; height: 100vh; display: grid; grid-template-rows: var(--top-menu-height) auto minmax(0, 1fr) 24px; color: var(--text); background: var(--bg); overflow: hidden; }
.workspace { position: relative; display: grid; grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, 1fr) min(var(--results-pane-height), 35vh); min-width: 0; min-height: 0; }
.app-shell.results-collapsed .workspace { grid-template-rows: minmax(0, 1fr) 28px; }
.hex-stage { position: relative; min-width: 0; min-height: 0; overflow: hidden; }
.search-notice { position: absolute; z-index: 2; top: 10px; left: 12px; padding: 5px 8px; color: var(--modified); background: color-mix(in srgb, var(--surface) 92%, transparent); border: 1px solid var(--border); border-radius: 4px; font-size: var(--font-support); pointer-events: none; }
.drop-prompt { position: absolute; inset: 0; display: grid; place-content: center; justify-items: center; gap: 7px; color: var(--muted); }
.drop-prompt span { display: grid; place-items: center; width: 42px; height: 42px; color: var(--address); border: 1px dashed var(--border-strong); border-radius: 8px; font-size: 22px; }
.empty-file { position: absolute; inset: 0 96px 0 0; display: grid; place-items: center; color: var(--muted); background: var(--bg); font-size: var(--font-body); pointer-events: none; }
.search-bar { position: absolute; z-index: 3; top: 7px; right: calc(var(--minimap-width, 40px) + 10px); width: min(390px, calc(100% - var(--minimap-width, 40px) - 20px)); box-sizing: border-box; padding: 7px; background: var(--panel); border: 1px solid var(--border-strong); border-radius: 5px; box-shadow: 0 5px 14px rgb(0 0 0 / 15%); }
.search-bar__controls { display: flex; align-items: center; gap: 5px; min-width: 0; }
.search-bar input { flex: 1; min-width: 50px; height: 27px; padding: 0 7px; color: var(--text); background: var(--input); border: 1px solid var(--border); border-radius: 3px; font: inherit; font-size: var(--font-body); outline: none; }
.search-bar input:focus { border-color: var(--selection); }
.search-bar button { height: 27px; padding: 0 7px; color: var(--text); background: var(--button); border: 0; border-radius: 3px; font: inherit; font-size: var(--font-support); white-space: nowrap; }
.search-bar button:hover:not(:disabled) { background: var(--hover); }
.search-bar button:disabled { opacity: .4; }
.search-bar .search-bar__close { width: 26px; padding: 0; color: var(--muted); background: transparent; font-size: 19px; }
.search-bar__detail { min-height: 15px; padding: 5px 2px 0; color: var(--muted); font-size: var(--font-support); }
.search-bar__detail--error { color: var(--modified); }
.visually-hidden { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
</style>
