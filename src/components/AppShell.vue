<script setup lang="ts">
import { computed } from 'vue'
import type { BytesPerRow } from '../hex/layout'
import type { ByteSelection } from '../hex/selection'
import type { Endian, FileInfo, PageRequest, ParsedField, TemplateDefinition, ViewportPage } from '../types'
import type { ColorTheme } from '../types'
import type { MenuCommand, MenuState } from '../menu/commands'
import AppDialog from './AppDialog.vue'
import FileInfoBar from './FileInfoBar.vue'
import HexCanvas from './HexCanvas.vue'
import ParsedResultsPanel from './ParsedResultsPanel.vue'
import StatusBar from './StatusBar.vue'
import TopMenu from './TopMenu.vue'

const props = withDefaults(defineProps<{
  file?: FileInfo | null; sourceIdentity?: number; page?: ViewportPage | null; bytesPerRow?: BytesPerRow; selection?: ByteSelection | null
  matches?: bigint[]; templateRange?: ByteSelection | null; editMode?: boolean; endianness?: Endian
  template?: TemplateDefinition; results?: ParsedField[]; dialogOpen?: boolean; dialogTitle?: string; dialogMessage?: string
  navigationOffset?: bigint
  matchLength?: number; busyLabel?: string; progressText?: string; searchTruncated?: boolean
  templateValid?: boolean; resultsNeedRefresh?: boolean
  menuState?: MenuState; theme?: ColorTheme; rightCollapsed?: boolean
}>(), {
  file: null, sourceIdentity: 0, page: null, bytesPerRow: 16, selection: null, matches: () => [], templateRange: null,
  editMode: false, endianness: 'little', template: () => ({ version: 1, name: 'Untitled', defaultEndianness: 'little', fields: [] }), results: () => [], dialogOpen: false,
  dialogTitle: '', dialogMessage: '',
  matchLength: 1, busyLabel: '', progressText: '', searchTruncated: false,
  templateValid: true, resultsNeedRefresh: false,
  menuState: () => ({ hasFile: false, hasBytes: false, singleByteSelected: false, editMode: false, canUndo: false, templateValid: true, templateActive: false, templateHasFields: false, hasNavigableTemplateFields: false, hasParsedResults: false, operationBusy: false }),
  theme: 'dark', rightCollapsed: false,
})

const emit = defineEmits<{
  command: [value: MenuCommand]
  'update:bytesPerRow': [value: BytesPerRow]; navigate: [range: { start: bigint; end: bigint }]
  'request-page': [request: PageRequest]; select: [selection: ByteSelection]; 'edit-request': [offset: bigint]
  'viewport-offset': [offset: bigint]; 'close-dialog': []
}>()

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
    :style="{ '--results-pane-height': '220px', '--top-menu-height': '34px' }"
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
      <ParsedResultsPanel :results="results" :collapsed="rightCollapsed" :has-file="Boolean(file)" :template-active="effectiveMenuState.templateActive"
        :template-name="template.name" :template-has-fields="template.fields.length > 0" :results-need-refresh="resultsNeedRefresh" :template-range="templateRange"
        @toggle-collapse="emit('command', 'toggle-right-panel')" @navigate="emit('navigate', $event)" @empty-action="emit('command', $event)" />
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
.drop-prompt button { height: 28px; margin-top: 3px; padding: 0 12px; color: var(--text); background: var(--button); border: 0; border-radius: 4px; font: inherit; font-size: var(--font-body); }
.drop-prompt button:hover { background: var(--hover); }
.empty-file { position: absolute; inset: 0 8px 0 0; display: grid; place-items: center; color: var(--muted); background: var(--bg); font-size: var(--font-body); pointer-events: none; }
</style>
