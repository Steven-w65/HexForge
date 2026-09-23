<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { BytesPerRow } from '../hex/layout'
import type { ByteSelection } from '../hex/selection'
import type { Endian, FileInfo, PageRequest, ParsedField, TemplateDefinition, ViewportPage } from '../types'
import type { ColorTheme } from '../types'
import { commandEnabled, type MenuCommand, type MenuState } from '../menu/commands'
import { COMPACT_RIGHT_WIDTH } from '../shell/layout'
import AppDialog from './AppDialog.vue'
import FileInfoBar from './FileInfoBar.vue'
import HexCanvas from './HexCanvas.vue'
import ParsedResultsPanel from './ParsedResultsPanel.vue'
import StatusBar from './StatusBar.vue'
import TemplateEditor from './TemplateEditor.vue'
import TopMenu from './TopMenu.vue'

const props = withDefaults(defineProps<{
  file?: FileInfo | null; sourceIdentity?: number; page?: ViewportPage | null; bytesPerRow?: BytesPerRow; selection?: ByteSelection | null
  matches?: bigint[]; templateRange?: ByteSelection | null; editMode?: boolean; endianness?: Endian
  template?: TemplateDefinition; results?: ParsedField[]; dialogOpen?: boolean; dialogTitle?: string; dialogMessage?: string
  navigationOffset?: bigint
  matchLength?: number; busyLabel?: string; progressText?: string; searchTruncated?: boolean
  templateValid?: boolean; templateEditorOpen?: boolean; templateDirty?: boolean
  menuState?: MenuState; theme?: ColorTheme; rightCollapsed?: boolean
}>(), {
  file: null, sourceIdentity: 0, page: null, bytesPerRow: 16, selection: null, matches: () => [], templateRange: null,
  editMode: false, endianness: 'little', template: () => ({ version: 1, name: 'Untitled', defaultEndianness: 'little', fields: [] }), results: () => [], dialogOpen: false,
  dialogTitle: '', dialogMessage: '',
  matchLength: 1, busyLabel: '', progressText: '', searchTruncated: false,
  templateValid: true, templateEditorOpen: false, templateDirty: false,
  menuState: () => ({ hasFile: false, hasBytes: false, singleByteSelected: false, editMode: false, canUndo: false, templateValid: true, templateHasFields: false, hasNavigableTemplateFields: false, hasParsedResults: false, operationBusy: false }),
  theme: 'dark', rightCollapsed: false,
})

const emit = defineEmits<{
  command: [value: MenuCommand]
  'update:bytesPerRow': [value: BytesPerRow]; 'update:template': [value: TemplateDefinition]
  'save-template': []; 'load-template': []; navigate: [range: { start: bigint; end: bigint }]
  'request-page': [request: PageRequest]; select: [selection: ByteSelection]; 'edit-request': [offset: bigint]
  'viewport-offset': [offset: bigint]; 'close-dialog': []
  'template-validity': [valid: boolean]
  'close-template-editor': []
}>()

const editorValid = ref(props.templateValid)
watch(() => props.templateValid, (valid) => { editorValid.value = valid })
function updateTemplateValidity(valid: boolean): void { editorValid.value = valid; emit('template-validity', valid) }
const fileSize = computed(() => BigInt(props.file?.size ?? '0'))
const effectiveMenuState = computed<MenuState>(() => ({
  ...props.menuState,
  templateValid: editorValid.value,
  templateHasFields: props.template.fields.length > 0,
  hasParsedResults: props.results.length > 0,
}))
const selectedByte = computed(() => {
  if (!props.selection || !props.page) return null
  const offset = props.selection.start
  const index = offset - BigInt(props.page.offset)
  if (index < 0n || index >= BigInt(props.page.bytes.length)) return null
  return { offset, value: props.page.bytes[Number(index)]! }
})

</script>

<template>
  <main
    data-testid="app-shell"
    class="app-shell"
    :class="{ 'right-collapsed': rightCollapsed }"
    :style="{ '--compact-right-width': `${COMPACT_RIGHT_WIDTH}px` }"
    role="application"
  >
    <TopMenu :state="effectiveMenuState" :bytes-per-row="bytesPerRow"
      :file-size="fileSize" :template="template" :results="results" @command="emit('command', $event)" @navigate="emit('navigate', $event)" />
    <FileInfoBar :file="file" />
    <div class="workspace">
      <section class="hex-stage">
        <HexCanvas v-if="file" :file-size="fileSize" :source-identity="sourceIdentity" :source-key="file.path" :source-revision="file.revision" :page="page" :bytes-per-row="bytesPerRow" :selection="selection" :matches="matches" :match-length="matchLength"
          :template-range="templateRange" :edit-mode="editMode" :theme="theme" :navigate-offset="navigationOffset" @request-page="emit('request-page', $event)"
          @select="emit('select', $event)" @edit-request="emit('edit-request', $event)" @viewport-offset="emit('viewport-offset', $event)" />
        <div v-else data-testid="drop-prompt" class="drop-prompt">
          <span>＋</span><strong>Open or drop a binary file to begin</strong>
          <button type="button" data-action="empty-open" @click="emit('command', 'open')">Open File</button>
        </div>
        <div v-if="file && fileSize === 0n" data-testid="empty-file" class="empty-file">This binary file is empty.</div>
        <div v-if="searchTruncated" data-testid="search-truncated" class="search-notice" role="status">Search results were limited; refine the byte pattern.</div>
      </section>
      <ParsedResultsPanel :results="results" :collapsed="rightCollapsed" :has-file="Boolean(file)" :template-has-fields="template.fields.length > 0"
        @toggle-collapse="emit('command', 'toggle-right-panel')" @navigate="emit('navigate', $event)" @empty-action="emit('command', $event)" />
    </div>
    <aside v-if="templateEditorOpen" data-testid="template-editor-panel" class="template-editor-panel" role="dialog" aria-modal="false" aria-label="Template Editor">
      <header class="template-editor-header">
        <div><strong>Template Editor</strong><small>{{ template.name || 'Untitled' }}<span v-if="templateDirty" data-testid="template-modified"> · Modified</span></small></div>
        <button type="button" data-action="close-template-editor" aria-label="Close Template Editor" @click="emit('close-template-editor')">×</button>
      </header>
      <TemplateEditor :model-value="template" :file-size="file ? fileSize : null" :results="results"
        :can-apply="commandEnabled('apply-template', effectiveMenuState)" @update:model-value="emit('update:template', $event)" @validity="updateTemplateValidity"
        @save="emit('save-template')" @load="emit('load-template')" @apply="emit('command', 'apply-template')" @navigate="emit('navigate', $event)" />
    </aside>
    <StatusBar :file-size="fileSize" :selected="selectedByte" :selected-count="selection?.count ?? 0n" :bytes-per-row="bytesPerRow"
      :edit-mode="editMode" :endianness="endianness" :dirty="file?.dirty ?? false" :busy-label="busyLabel" :progress-text="progressText"
      @update:bytes-per-row="emit('update:bytesPerRow', $event)" />
    <AppDialog :open="dialogOpen" :title="dialogTitle" :message="dialogMessage" @close="emit('close-dialog')"><slot name="dialog" /></AppDialog>
  </main>
</template>

<style scoped>
.app-shell { position: relative; width: 100vw; height: 100vh; display: grid; grid-template-rows: 28px auto minmax(0, 1fr) 24px; color: var(--text); background: var(--bg); overflow: hidden; }
.workspace { display: grid; grid-template-columns: minmax(360px, 1fr) minmax(240px, 30vw); min-width: 0; min-height: 0; }
.app-shell.right-collapsed .workspace { grid-template-columns: minmax(360px, 1fr) 28px; }
.hex-stage { position: relative; min-width: 0; min-height: 0; overflow: hidden; }
.search-notice { position: absolute; z-index: 2; top: 10px; left: 12px; padding: 5px 8px; color: var(--modified); background: color-mix(in srgb, var(--surface) 92%, transparent); border: 1px solid var(--border); border-radius: 4px; font-size: 10px; pointer-events: none; }
.drop-prompt { position: absolute; inset: 0; display: grid; place-content: center; justify-items: center; gap: 7px; color: var(--muted); }
.drop-prompt span { display: grid; place-items: center; width: 42px; height: 42px; color: var(--address); border: 1px dashed var(--border-strong); border-radius: 8px; font-size: 22px; }
.drop-prompt button { height: 28px; margin-top: 3px; padding: 0 12px; color: var(--text); background: var(--button); border: 0; border-radius: 4px; font: inherit; font-size: 10px; }
.drop-prompt button:hover { background: var(--hover); }
.empty-file { position: absolute; inset: 0 8px 0 0; display: grid; place-items: center; color: var(--muted); background: var(--bg); font-size: 11px; pointer-events: none; }
.template-editor-panel { position: absolute; z-index: 5; top: 70px; right: 0; bottom: 24px; box-sizing: border-box; width: min(560px, calc(100vw - 36px)); display: flex; flex-direction: column; gap: 12px; padding: 12px; color: var(--text); background: color-mix(in srgb, var(--panel) 97%, transparent); border-left: 1px solid var(--border-strong); box-shadow: -12px 0 28px rgb(0 0 0 / 16%); }
.template-editor-header { display: flex; align-items: center; justify-content: space-between; padding-bottom: 9px; border-bottom: 1px solid var(--border); }
.template-editor-header div { display: grid; gap: 2px; }
.template-editor-header strong { font-size: 12px; }
.template-editor-header small { color: var(--muted); font-size: 10px; }
.template-editor-header small span { color: var(--modified); }
.template-editor-header button { width: 26px; height: 26px; color: var(--muted); background: transparent; border: 0; border-radius: 4px; font: inherit; font-size: 18px; }
.template-editor-header button:hover { color: var(--text); background: var(--hover); }
@media (max-width: 760px) { .workspace { grid-template-columns: minmax(260px, 1fr) 28px; } }
@media (max-width: 1280px) and (min-width: 761px) { .workspace { grid-template-columns: minmax(0, 1fr) var(--compact-right-width); } }
</style>
