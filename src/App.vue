<script setup lang="ts">
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { confirm, message, open, save } from '@tauri-apps/plugin-dialog'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import AppShell from './components/AppShell.vue'
import { useHexSession } from './composables/useHexSession'
import { handleCloseRequest, useHotkeys } from './composables/useHotkeys'
import type { BytesPerRow } from './hex/layout'
import type { TemplateDefinition } from './types'
import { normalizeSelection, type ByteSelection } from './hex/selection'

type PromptKind = 'goto' | 'search' | 'edit'
const session = useHexSession()
const bytesPerRow = ref<BytesPerRow>(16)
const popup = ref<{ kind: PromptKind; title: string; value: string } | null>(null)
const templateRange = ref<ByteSelection | null>(null)
const allowClose = { value: false }
const disposers: Array<() => void> = []
let disposed = false

async function reportFailure(operation: () => Promise<void>): Promise<void> {
  try { await operation() }
  catch { /* useHexSession exposes the normalized failure through AppDialog */ }
}

async function nativeCall<T>(operation: () => Promise<T>): Promise<T | undefined> {
  try { return await operation() }
  catch (error) { session.presentError(error); return undefined }
}

async function confirmDiscard(): Promise<boolean> {
  return await nativeCall(() => confirm('This file has unsaved in-memory edits. Discard them?', { title: 'HexForge', kind: 'warning' })) === true
}

async function openPath(path: string): Promise<void> {
  let discard = false
  if (session.file.value?.dirty) {
    discard = await confirmDiscard()
    if (!discard) return
  }
  await reportFailure(() => session.openFile(path, discard))
}

async function chooseFile(): Promise<void> {
  const selected = await nativeCall(() => open({ multiple: false, directory: false, title: 'Open binary file' }))
  if (typeof selected === 'string') await openPath(selected)
}

async function chooseSaveAs(): Promise<void> {
  const path = await nativeCall(() => save({ title: 'Save binary as', defaultPath: session.file.value ? `${session.file.value.name}.copy` : undefined }))
  if (path) await reportFailure(() => session.saveAs(path))
}

async function chooseTemplateLoad(): Promise<void> {
  const path = await nativeCall(() => open({ multiple: false, directory: false, title: 'Load parsing template', filters: [{ name: 'JSON template', extensions: ['json'] }] }))
  if (typeof path === 'string') await reportFailure(() => session.loadTemplate(path))
}

async function chooseTemplateSave(): Promise<void> {
  const path = await nativeCall(() => save({ title: 'Save parsing template', defaultPath: `${session.template.value.name || 'template'}.json`, filters: [{ name: 'JSON template', extensions: ['json'] }] }))
  if (path) await reportFailure(() => session.saveTemplate(path))
}

async function chooseCsvExport(): Promise<void> {
  const path = await nativeCall(() => save({ title: 'Export parsed results', defaultPath: `${session.template.value.name || 'results'}.csv`, filters: [{ name: 'CSV', extensions: ['csv'] }] }))
  if (path) await reportFailure(() => session.exportCsv(path))
}

function showPrompt(kind: PromptKind, title: string, value = ''): void { popup.value = { kind, title, value } }
function closePopup(): void { popup.value = null; session.clearError() }

async function submitPrompt(): Promise<void> {
  const active = popup.value
  if (!active) return
  try {
    if (active.kind === 'goto') session.goTo(active.value)
    else if (active.kind === 'search') await session.search(active.value)
    else await session.editSelectedByte(active.value)
    popup.value = null
  } catch { /* validation is rendered in the in-app dialog */ }
}

function beginEdit(offset: bigint): void {
  session.selection.value = { start: offset, end: offset, count: 1n }
  showPrompt('edit', 'Edit byte', '')
}

function updateTemplate(value: TemplateDefinition): void { session.updateTemplate(value) }
function navigateTemplate(range: { start: bigint; end: bigint }): void {
  templateRange.value = normalizeSelection(range.start, range.end)
  session.navigate(range)
}
function selectBytes(value: ByteSelection): void { templateRange.value = null; session.selection.value = value }

const activeBusy = computed(() => {
  const labels: Array<[keyof typeof session.busy, string]> = [
    ['open', 'Opening file'], ['search', 'Searching bytes'], ['parse', 'Parsing template'], ['save', 'Saving copy'],
    ['export', 'Exporting CSV'], ['template', 'Working with template'], ['edit', 'Applying edit'], ['undo', 'Undoing edit'], ['page', 'Loading bytes'],
  ]
  const operation = session.activity.value?.operation
  return operation ? labels.find(([name]) => name === operation)?.[1] ?? '' : ''
})
const progressText = computed(() => session.activity.value?.progress
  ? `${session.activity.value.progress.processed} / ${session.activity.value.progress.total}`
  : '')

onMounted(async () => {
  disposers.push(useHotkeys({
    open: () => { void chooseFile() }, search: () => showPrompt('search', 'Search bytes'), goTo: () => showPrompt('goto', 'Go to offset'),
    saveTemplate: () => { void chooseTemplateSave() }, undo: () => { if (session.file.value) void reportFailure(session.undo) },
    isPopupOpen: () => popup.value !== null || session.error.value !== null,
    closePopup, clearSelection: session.clearSelection,
  }))
  try {
    const closeUnlisten = await getCurrentWindow().onCloseRequested(async (event) => {
      try { await handleCloseRequest(event, session.file.value?.dirty ?? false, confirmDiscard, () => getCurrentWindow().close(), allowClose) }
      catch (error) { session.presentError(error) }
    })
    if (disposed) closeUnlisten(); else disposers.push(closeUnlisten)
    const dropUnlisten = await getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === 'drop' && event.payload.paths.length === 1) void openPath(event.payload.paths[0]!)
      else if (event.payload.type === 'drop') void nativeCall(() => message('Drop exactly one file at a time.', { title: 'HexForge', kind: 'warning' }))
    })
    if (disposed) dropUnlisten(); else disposers.push(dropUnlisten)
  } catch (error) {
    session.presentError(error)
    await nativeCall(() => message(error instanceof Error ? error.message : 'Desktop listeners could not be registered.', { title: 'HexForge', kind: 'error' }))
  }
})

onBeforeUnmount(() => { disposed = true; disposers.splice(0).forEach((dispose) => dispose()) })
</script>

<template>
  <AppShell
    data-testid="hexforge-app" :file="session.file.value" :page="session.page.value" :selection="session.selection.value"
    :template="session.template.value" :results="session.results.value" :matches="session.matches.value"
    :match-length="session.searchMatchLength.value" :search-truncated="session.searchTruncated.value" :template-range="templateRange"
    :busy-label="activeBusy" :progress-text="progressText"
    :bytes-per-row="bytesPerRow" :edit-mode="session.editMode.value" :endianness="session.template.value.defaultEndianness"
    :navigation-offset="session.viewportOffset.value" :dialog-open="popup !== null || session.error.value !== null"
    :dialog-title="popup?.title ?? (session.error.value ? 'Operation failed' : '')" :dialog-message="session.error.value?.message ?? ''"
    @open="chooseFile" @goto="showPrompt('goto', 'Go to offset')" @search="showPrompt('search', 'Search bytes')"
    @template="reportFailure(session.applyTemplate)" @export="chooseCsvExport" @edit="session.editMode.value = !session.editMode.value"
    @save-as="chooseSaveAs" @update:bytes-per-row="bytesPerRow = $event" @update:template="updateTemplate"
    @save-template="chooseTemplateSave" @load-template="chooseTemplateLoad" @navigate="navigateTemplate"
    @request-page="reportFailure(() => session.requestPage($event.offset, $event.length, $event.generation))" @select="selectBytes"
    @edit-request="beginEdit" @viewport-offset="session.viewportOffset.value = $event" @close-dialog="closePopup"
  >
    <template #dialog>
      <form v-if="popup" class="prompt-form" @submit.prevent="submitPrompt">
        <input v-model="popup.value" autofocus :placeholder="popup.kind === 'search' ? '41 42 43' : popup.kind === 'goto' ? '0x100' : 'FF'">
        <button type="submit">OK</button>
      </form>
    </template>
  </AppShell>
</template>
<style src="./styles/theme.css"></style>
<style scoped>
.prompt-form { display: flex; gap: 8px; margin-top: 12px; }
.prompt-form input { flex: 1; min-width: 0; height: 28px; padding: 0 8px; color: var(--text); background: var(--input); border: 1px solid var(--border); border-radius: 3px; font: inherit; }
.prompt-form button { padding: 0 14px; color: var(--text); background: var(--button); border: 0; border-radius: 3px; font: inherit; }
</style>
