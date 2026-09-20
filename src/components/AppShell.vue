<script setup lang="ts">
import { computed, ref } from 'vue'
import type { BytesPerRow } from '../hex/layout'
import type { ByteSelection } from '../hex/selection'
import type { Endian, FileInfo, PageRequest, ParsedField, TemplateDefinition, ViewportPage } from '../types'
import { useTheme } from '../composables/useTheme'
import { COMPACT_LEFT_WIDTH, COMPACT_RIGHT_WIDTH } from '../shell/layout'
import AppDialog from './AppDialog.vue'
import HexCanvas from './HexCanvas.vue'
import ParsedResultsPanel from './ParsedResultsPanel.vue'
import SidebarPanel from './SidebarPanel.vue'
import StatusBar from './StatusBar.vue'
import TopToolbar from './TopToolbar.vue'

const props = withDefaults(defineProps<{
  file?: FileInfo | null; page?: ViewportPage | null; bytesPerRow?: BytesPerRow; selection?: ByteSelection | null
  matches?: bigint[]; templateRange?: ByteSelection | null; editMode?: boolean; endianness?: Endian
  template?: TemplateDefinition; results?: ParsedField[]; dialogOpen?: boolean; dialogTitle?: string; dialogMessage?: string
  navigationOffset?: bigint
  matchLength?: number; busyLabel?: string; progressText?: string; searchTruncated?: boolean
}>(), {
  file: null, page: null, bytesPerRow: 16, selection: null, matches: () => [], templateRange: null,
  editMode: false, endianness: 'little', template: () => ({ version: 1, name: 'Untitled', defaultEndianness: 'little', fields: [] }), results: () => [], dialogOpen: false,
  dialogTitle: '', dialogMessage: '',
  matchLength: 1, busyLabel: '', progressText: '', searchTruncated: false,
})

const emit = defineEmits<{
  open: []; goto: []; search: []; template: []; export: []; theme: []; edit: []; 'save-as': []
  'update:bytesPerRow': [value: BytesPerRow]; 'update:template': [value: TemplateDefinition]
  'save-template': []; 'load-template': []; navigate: [range: { start: bigint; end: bigint }]
  'request-page': [request: PageRequest]; select: [selection: ByteSelection]; 'edit-request': [offset: bigint]
  'viewport-offset': [offset: bigint]; 'close-dialog': []
}>()

const leftCollapsed = ref(false)
const rightCollapsed = ref(false)
const theme = useTheme()
const fileSize = computed(() => BigInt(props.file?.size ?? '0'))
const selectedByte = computed(() => {
  if (!props.selection || !props.page) return null
  const offset = props.selection.start
  const index = offset - BigInt(props.page.offset)
  if (index < 0n || index >= BigInt(props.page.bytes.length)) return null
  return { offset, value: props.page.bytes[Number(index)]! }
})

function toolbarAction(action: 'open' | 'goto' | 'search' | 'template' | 'export' | 'theme' | 'edit' | 'save-as'): void {
  if (action === 'theme') theme.toggle()
  switch (action) {
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
  <main
    data-testid="app-shell"
    class="app-shell"
    :class="{ 'left-collapsed': leftCollapsed, 'right-collapsed': rightCollapsed }"
    :style="{ '--compact-left-width': `${COMPACT_LEFT_WIDTH}px`, '--compact-right-width': `${COMPACT_RIGHT_WIDTH}px` }"
    role="application"
  >
    <TopToolbar :has-file="Boolean(file)" :dirty="file?.dirty ?? false" :edit-mode="editMode" :theme="theme.value.value"
      @open="toolbarAction('open')" @goto="toolbarAction('goto')" @search="toolbarAction('search')" @template="toolbarAction('template')"
      @export="toolbarAction('export')" @theme="toolbarAction('theme')" @edit="toolbarAction('edit')" @save-as="toolbarAction('save-as')" />
    <div class="workspace">
      <SidebarPanel :file="file" :template="template" :collapsed="leftCollapsed" @toggle-collapse="leftCollapsed = !leftCollapsed"
        @update:template="emit('update:template', $event)" @save-template="emit('save-template')" @load-template="emit('load-template')" @navigate="emit('navigate', $event)" />
      <section class="hex-stage">
        <HexCanvas v-if="file" :file-size="fileSize" :page="page" :bytes-per-row="bytesPerRow" :selection="selection" :matches="matches" :match-length="matchLength"
          :template-range="templateRange" :edit-mode="editMode" :theme="theme.value.value" :navigate-offset="navigationOffset" @request-page="emit('request-page', $event)"
          @select="emit('select', $event)" @edit-request="emit('edit-request', $event)" @viewport-offset="emit('viewport-offset', $event)" />
        <div v-else data-testid="drop-prompt" class="drop-prompt"><span>＋</span><strong>Drop a binary file here</strong><small>or use Open File</small></div>
        <div v-if="busyLabel" data-testid="operation-status" class="operation-status" role="status">{{ busyLabel }}<span v-if="progressText"> · {{ progressText }}</span></div>
        <div v-if="searchTruncated" data-testid="search-truncated" class="search-notice" role="status">Search results were limited; refine the byte pattern.</div>
      </section>
      <ParsedResultsPanel :results="results" :collapsed="rightCollapsed" @toggle-collapse="rightCollapsed = !rightCollapsed" @navigate="emit('navigate', $event)" />
    </div>
    <StatusBar :file-size="fileSize" :selected="selectedByte" :selected-count="selection?.count ?? 0n" :bytes-per-row="bytesPerRow"
      :edit-mode="editMode" :endianness="endianness" :dirty="file?.dirty ?? false" @update:bytes-per-row="emit('update:bytesPerRow', $event)" />
    <AppDialog :open="dialogOpen" :title="dialogTitle" :message="dialogMessage" @close="emit('close-dialog')"><slot name="dialog" /></AppDialog>
  </main>
</template>

<style scoped>
.app-shell { width: 100vw; height: 100vh; display: grid; grid-template-rows: 42px minmax(0, 1fr) 24px; color: var(--text); background: var(--bg); overflow: hidden; }
.workspace { display: grid; grid-template-columns: minmax(210px, 260px) minmax(360px, 1fr) minmax(280px, 34vw); min-width: 0; min-height: 0; }
.app-shell.left-collapsed .workspace { grid-template-columns: 28px minmax(360px, 1fr) minmax(280px, 34vw); }
.app-shell.right-collapsed .workspace { grid-template-columns: minmax(210px, 260px) minmax(360px, 1fr) 28px; }
.app-shell.left-collapsed.right-collapsed .workspace { grid-template-columns: 28px minmax(360px, 1fr) 28px; }
.hex-stage { position: relative; min-width: 0; min-height: 0; overflow: hidden; }
.operation-status, .search-notice { position: absolute; z-index: 2; left: 12px; padding: 5px 8px; color: var(--text); background: color-mix(in srgb, var(--surface) 92%, transparent); border: 1px solid var(--border); border-radius: 4px; font-size: 10px; pointer-events: none; }
.operation-status { top: 10px; }
.search-notice { top: 42px; color: var(--modified); }
.drop-prompt { position: absolute; inset: 0; display: grid; place-content: center; justify-items: center; gap: 7px; color: var(--muted); }
.drop-prompt span { display: grid; place-items: center; width: 42px; height: 42px; color: var(--address); border: 1px dashed var(--border-strong); border-radius: 8px; font-size: 22px; }
.drop-prompt small { font-size: 10px; }
@media (max-width: 760px) { .workspace, .app-shell.left-collapsed .workspace { grid-template-columns: 28px minmax(260px, 1fr) 28px; } }
@media (max-width: 1280px) and (min-width: 761px) { .workspace { grid-template-columns: var(--compact-left-width) minmax(0, 1fr) var(--compact-right-width); } }
</style>
