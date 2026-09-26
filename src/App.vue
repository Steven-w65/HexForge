<script setup lang="ts">
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { confirm, message, open, save } from '@tauri-apps/plugin-dialog'
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import AppShell from './components/AppShell.vue'
import { useHexSession } from './composables/useHexSession'
import { handleCloseRequest, useHotkeys } from './composables/useHotkeys'
import { useTheme } from './composables/useTheme'
import type { BytesPerRow } from './hex/layout'
import { commandEnabled, type MenuCommand, type MenuState } from './menu/commands'
import type { TemplateDefinition, TemplateField } from './types'
import { normalizeSelection, type ByteSelection } from './hex/selection'
import { createMainBridge } from './templateWindow/mainBridge'
import { tauriBus } from './templateWindow/tauriBus'
import { templateWindowManager } from './templateWindow/windowManager'
import type { EditorAction } from './templateWindow/protocol'

type PromptKind = 'goto' | 'search' | 'edit'
const session = useHexSession()
const bytesPerRow = ref<BytesPerRow>(16)
const RIGHT_PANEL_KEY = 'hexforge.rightCollapsed'
function readPanelPreference(key: string, fallback: boolean): boolean {
  try {
    const value = localStorage.getItem(key)
    return value === null ? fallback : value === 'true'
  } catch { return fallback }
}
function storePanelPreference(key: string, value: boolean): void {
  try { localStorage.setItem(key, String(value)) } catch { /* preference storage is optional */ }
}

const rightCollapsed = ref(readPanelPreference(RIGHT_PANEL_KEY, false))
const theme = useTheme()
const popup = ref<{ kind: PromptKind; title: string; value: string } | null>(null)
const templateRange = ref<ByteSelection | null>(null)
const templateValid = ref(true)
const closeGuard = { confirming: false }
const disposers: Array<() => void> = []
let disposed = false

function templateSnapshot(value: TemplateDefinition): string { return JSON.stringify(value) }
const templateBaseline = ref(templateSnapshot(session.template.value))
const templateDirty = computed(() => templateSnapshot(session.template.value) !== templateBaseline.value)
const templateActive = ref(false)

async function reportFailure(operation: () => Promise<void>): Promise<boolean> {
  try { await operation(); return true }
  catch (error) { session.presentError(error); return false }
}

async function nativeCall<T>(operation: () => Promise<T>): Promise<T | undefined> {
  try { return await operation() }
  catch (error) { session.presentError(error); return undefined }
}

async function confirmDiscard(): Promise<boolean> {
  return await nativeCall(() => confirm('This file has unsaved in-memory edits. Discard them?', { title: 'HexForge', kind: 'warning' })) === true
}

async function confirmExitDiscard(fileDirty: boolean, templateDraftDirty: boolean): Promise<boolean> {
  const description = fileDirty && templateDraftDirty
    ? 'Unsaved byte edits and template changes will be lost. Exit HexForge?'
    : templateDraftDirty
      ? 'Unsaved template changes will be lost. Exit HexForge?'
      : 'Unsaved byte edits will be lost. Exit HexForge?'
  return await nativeCall(() => confirm(description, { title: 'HexForge', kind: 'warning' })) === true
}

async function openPath(path: string): Promise<void> {
  let discard = false
  if (session.file.value?.dirty) {
    discard = await confirmDiscard()
    if (!discard) return
  }
  if (await reportFailure(() => session.openFile(path, discard))) clearTemplateHighlight()
}

async function chooseFile(): Promise<void> {
  const selected = await nativeCall(() => open({ multiple: false, directory: false, title: 'Open binary file' }))
  if (typeof selected === 'string') await openPath(selected)
}

async function chooseSaveAs(): Promise<void> {
  const path = await nativeCall(() => save({ title: 'Save binary as', defaultPath: session.file.value ? `${session.file.value.name}.copy` : undefined }))
  if (path) await reportFailure(() => session.saveAs(path))
}

async function closeCurrentFile(): Promise<void> {
  if (!session.file.value) return
  let discard = false
  if (session.file.value.dirty) {
    discard = await confirmDiscard()
    if (!discard) return
  }
  await reportFailure(() => session.closeFile(discard))
  if (!session.file.value) clearTemplateHighlight()
}

async function exitApplication(): Promise<void> {
  await nativeCall(() => getCurrentWindow().close())
}

async function inTemplateDialog<T>(fromEditor: boolean, operation: () => Promise<T>): Promise<T> {
  if (!fromEditor) return operation()
  // Native dialogs belong to the main window; bring it forward when the companion requested one.
  await getCurrentWindow().setFocus()
  try { return await operation() }
  finally {
    try { if (bridge.isReady()) await templateWindowManager.openOrFocus() }
    catch (error) { session.presentError(error) }
  }
}

async function chooseTemplateLoad(flushDraft = true): Promise<void> {
  if (flushDraft) await bridge.flush()
  await inTemplateDialog(!flushDraft, async () => {
    const path = await open({ multiple: false, directory: false, title: 'Load parsing template', filters: [{ name: 'JSON template', extensions: ['json'] }] })
    if (typeof path !== 'string') return
    if (templateDirty.value && !await confirm('This template has unsaved changes. Discard them and load another template?', { title: 'HexForge', kind: 'warning' })) return
    await session.loadTemplate(path)
    clearTemplateHighlight()
    templateBaseline.value = templateSnapshot(session.template.value)
    templateActive.value = true
    if (!bridge.isReady()) await openTemplateEditor()
  })
}

async function chooseTemplateSave(flushDraft = true): Promise<void> {
  if (flushDraft) await bridge.flush()
  if (!templateValid.value) { throw { code: 'invalid_template', message: 'Correct the highlighted template field before saving.' } }
  await inTemplateDialog(!flushDraft, async () => {
    const path = await save({ title: 'Save parsing template', defaultPath: `${session.template.value.name || 'template'}.json`, filters: [{ name: 'JSON template', extensions: ['json'] }] })
    if (!path) return
    await session.saveTemplate(path)
    templateBaseline.value = templateSnapshot(session.template.value)
    templateActive.value = true
  })
}

async function chooseCsvExport(): Promise<void> {
  const path = await nativeCall(() => save({ title: 'Export parsed results', defaultPath: `${session.template.value.name || 'results'}.csv`, filters: [{ name: 'CSV', extensions: ['csv'] }] }))
  if (path) await reportFailure(() => session.exportCsv(path))
}

function showPrompt(kind: PromptKind, title: string, value = ''): void { popup.value = { kind, title, value } }
function closePopup(): void { popup.value = null; session.clearError() }
function closeTopLayer(): void {
  if (popup.value !== null || session.error.value !== null) closePopup()
}

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

function clearTemplateHighlight(): void {
  const range = templateRange.value
  if (!range) return
  const selection = session.selection.value
  if (selection?.start === range.start && selection.end === range.end) session.clearSelection()
  templateRange.value = null
}

function updateTemplate(value: TemplateDefinition): void {
  session.updateTemplate(value)
  clearTemplateHighlight()
  templateActive.value = true
}

async function unloadTemplate(flushDraft = true): Promise<void> {
  if (flushDraft) await bridge.flush()
  if (templateDirty.value) {
    const discard = await inTemplateDialog(!flushDraft, () => confirm('This template has unsaved changes. Discard them and unload it?', { title: 'HexForge', kind: 'warning' }))
    if (discard !== true) return
  }
  session.unloadTemplate()
  templateBaseline.value = templateSnapshot(session.template.value)
  templateActive.value = false
  clearTemplateHighlight()
  templateValid.value = true
}
function templateFieldLength(field: TemplateField): bigint {
  if (field.type === 'string' || field.type === 'bytes') return BigInt(field.length ?? 1)
  if (field.type.endsWith('8')) return 1n
  if (field.type.endsWith('16')) return 2n
  if (field.type.endsWith('32') || field.type === 'f32') return 4n
  return 8n
}
function nextTemplateOffset(): string {
  let next = 0n
  for (const field of session.template.value.fields) {
    try {
      const offset = BigInt(field.offset)
      const end = offset + templateFieldLength(field)
      if (end > next) next = end
    } catch { /* invalid drafts are handled by the editor */ }
  }
  return next.toString()
}
function addTemplateField(): void {
  const fields = session.template.value.fields
  updateTemplate({ ...session.template.value, fields: [...fields, {
    name: `field${fields.length + 1}`, offset: nextTemplateOffset(), type: 'u8', endianness: session.template.value.defaultEndianness, comment: '',
  }] })
  void openTemplateEditor()
}
async function applyValidTemplate(flushDraft = true): Promise<void> {
  if (flushDraft) await bridge.flush()
  if (!templateValid.value) { throw { code: 'invalid_template', message: 'Correct the highlighted template field before applying.' } }
  await session.applyTemplate()
}
function navigateTemplate(range: { start: bigint; end: bigint }): void {
  templateRange.value = normalizeSelection(range.start, range.end)
  session.navigate(range)
}
function selectBytes(value: ByteSelection): void { templateRange.value = null; session.selection.value = value }

const activeBusy = computed(() => {
  const labels: Array<[keyof typeof session.busy, string]> = [
    ['open', 'Opening file'], ['close', 'Closing file'], ['search', 'Searching bytes'], ['parse', 'Parsing template'], ['save', 'Saving copy'],
    ['export', 'Exporting CSV'], ['template', 'Working with template'], ['edit', 'Applying edit'], ['undo', 'Undoing edit'], ['page', 'Loading bytes'],
  ]
  const operation = session.activity.value?.operation
  return operation ? labels.find(([name]) => name === operation)?.[1] ?? '' : ''
})
const progressText = computed(() => session.activity.value?.progress
  ? `${session.activity.value.progress.processed} / ${session.activity.value.progress.total}`
  : '')

const menuState = computed<MenuState>(() => ({
  hasFile: session.file.value !== null,
  hasBytes: BigInt(session.file.value?.size ?? '0') > 0n,
  singleByteSelected: session.selection.value?.count === 1n,
  editMode: session.editMode.value,
  canUndo: session.canUndo.value,
  templateValid: templateValid.value,
  templateActive: templateActive.value,
  templateHasFields: session.template.value.fields.length > 0,
  hasNavigableTemplateFields: session.template.value.fields.length > 0,
  hasParsedResults: session.results.value.length > 0,
  operationBusy: session.activity.value !== null,
}))

const bridge = createMainBridge(tauriBus, {
  snapshot: () => ({
    template: session.template.value, results: session.results.value, fileSize: session.file.value?.size ?? null,
    theme: theme.value.value, dirty: templateDirty.value, canApply: commandEnabled('apply-template', menuState.value), active: templateActive.value,
  }),
  onDraft: (draft) => {
    if (templateSnapshot(session.template.value) !== templateSnapshot(draft.template)) updateTemplate(draft.template)
    templateValid.value = draft.valid
  },
  onAction: async (action: EditorAction) => {
    switch (action.command) {
      case 'load': await chooseTemplateLoad(false); break
      case 'save': await chooseTemplateSave(false); break
      case 'apply': await applyValidTemplate(false); break
      case 'unload': await unloadTemplate(false); break
      case 'navigate': if (action.range) navigateTemplate({ start: BigInt(action.range.start), end: BigInt(action.range.end) }); break
      case 'close': break // The accepted draft remains in the authoritative main session.
    }
  },
  onError: (error) => session.presentError(error),
  onClosed: () => templateWindowManager.forget(),
})

watch(() => [session.template.value, session.results.value, session.file.value?.size, theme.value.value,
  templateDirty.value, templateActive.value, templateValid.value, menuState.value.operationBusy],
  () => { void bridge.publish().catch((error) => session.presentError(error)) }, { flush: 'post' })

async function openTemplateEditor(): Promise<void> {
  try { await bridge.start(); await templateWindowManager.openOrFocus() }
  catch (error) { session.presentError(error) }
}

function executeCommand(command: MenuCommand): void {
  if (!commandEnabled(command, menuState.value)) return
  switch (command) {
    case 'open': void chooseFile(); break
    case 'close-file': void closeCurrentFile(); break
    case 'save-as': void chooseSaveAs(); break
    case 'export': void chooseCsvExport(); break
    case 'exit': void exitApplication(); break
    case 'edit-selected': {
      const selected = session.selection.value
      if (selected) beginEdit(selected.start)
      break
    }
    case 'toggle-edit': session.editMode.value = !session.editMode.value; break
    case 'undo': void reportFailure(session.undo); break
    case 'goto': showPrompt('goto', 'Go to offset'); break
    case 'search': showPrompt('search', 'Search bytes'); break
    case 'template-editor': void openTemplateEditor(); break
    case 'apply-template': void reportFailure(applyValidTemplate); break
    case 'load-template': void reportFailure(chooseTemplateLoad); break
    case 'unload-template': void reportFailure(unloadTemplate); break
    case 'save-template': void reportFailure(chooseTemplateSave); break
    case 'add-field': addTemplateField(); break
    case 'theme-toggle': theme.toggle(); break
    case 'row-16': bytesPerRow.value = 16; break
    case 'row-32': bytesPerRow.value = 32; break
    case 'toggle-right-panel': rightCollapsed.value = !rightCollapsed.value; storePanelPreference(RIGHT_PANEL_KEY, rightCollapsed.value); break
  }
}

onMounted(async () => {
  try { await bridge.start() } catch (error) { session.presentError(error) }
  disposers.push(useHotkeys({
    invoke: executeCommand, isEnabled: (command) => commandEnabled(command, menuState.value),
    isPopupOpen: () => popup.value !== null || session.error.value !== null,
    closePopup: closeTopLayer, clearSelection: session.clearSelection,
  }))
  try {
    const closeUnlisten = await getCurrentWindow().onCloseRequested(async (event) => {
      let fileDirty = false
      let templateDraftDirty = false
      try {
        await handleCloseRequest(event, async () => {
          await bridge.flush()
          const state = await session.prepareClose()
          fileDirty = state.dirty
          templateDraftDirty = templateDirty.value
          return { dirty: fileDirty || templateDraftDirty }
        }, () => confirmExitDiscard(fileDirty, templateDraftDirty), closeGuard, session.releaseCloseBarrier)
      }
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

onBeforeUnmount(() => { disposed = true; bridge.dispose(); disposers.splice(0).forEach((dispose) => dispose()) })
</script>

<template>
  <AppShell
    data-testid="hexforge-app" :file="session.file.value" :source-identity="session.sourceIdentity.value" :page="session.page.value" :selection="session.selection.value"
    :template="session.template.value" :results="session.results.value" :matches="session.matches.value"
    :match-length="session.searchMatchLength.value" :search-truncated="session.searchTruncated.value" :template-range="templateRange"
    :busy-label="activeBusy" :progress-text="progressText"
    :template-valid="templateValid" :results-need-refresh="session.resultsNeedRefresh.value"
    :menu-state="menuState" :theme="theme.value.value" :right-collapsed="rightCollapsed"
    :bytes-per-row="bytesPerRow" :edit-mode="session.editMode.value" :endianness="session.template.value.defaultEndianness"
    :navigation-offset="session.viewportOffset.value" :dialog-open="popup !== null || session.error.value !== null"
    :dialog-title="popup?.title ?? (session.error.value ? 'Operation failed' : '')" :dialog-message="session.error.value?.message ?? ''"
    @command="executeCommand" @update:bytes-per-row="bytesPerRow = $event" @navigate="navigateTemplate"
    @request-page="reportFailure(() => session.requestPage($event.offset, $event.length, $event.generation))" @select="selectBytes"
    @edit-request="beginEdit" @viewport-offset="session.viewportOffset.value = $event" @close-dialog="closePopup"
  >
    <template #dialog>
      <form v-if="popup" class="prompt-form" @submit.prevent="submitPrompt">
        <label for="prompt-value">{{ popup.kind === 'search' ? 'Hex byte sequence' : popup.kind === 'goto' ? 'Offset' : 'Hex byte value' }}</label>
        <input id="prompt-value" v-model="popup.value" autofocus :placeholder="popup.kind === 'search' ? '41 42 43' : popup.kind === 'goto' ? '0x100' : 'FF'">
        <small>{{ popup.kind === 'search' ? 'Enter space-separated hexadecimal bytes.' : popup.kind === 'goto' ? 'Enter a decimal value or a 0x-prefixed hexadecimal offset.' : 'Enter one hexadecimal byte from 00 to FF.' }}</small>
        <div class="prompt-actions"><button type="button" class="secondary" @click="closePopup">Cancel</button><button type="submit">{{ popup.kind === 'search' ? 'Search' : popup.kind === 'goto' ? 'Go' : 'Apply' }}</button></div>
      </form>
    </template>
  </AppShell>
</template>
<style scoped>
.prompt-form { display: grid; gap: 7px; margin-top: 12px; }
.prompt-form label { color: var(--text); font-size: var(--font-support); }
.prompt-form small { color: var(--muted); font-size: var(--font-support); line-height: 1.4; }
.prompt-form input { min-width: 0; height: 30px; padding: 0 8px; color: var(--text); background: var(--input); border: 1px solid var(--border); border-radius: 4px; outline: none; font: inherit; }
.prompt-form input:focus { border-color: var(--selection); box-shadow: 0 0 0 1px color-mix(in srgb, var(--selection) 55%, transparent); }
.prompt-actions { display: flex; justify-content: flex-end; gap: 7px; margin-top: 5px; }
.prompt-form button { height: 28px; padding: 0 14px; color: var(--text); background: var(--button); border: 0; border-radius: 4px; font: inherit; }
.prompt-form button:hover { background: var(--hover); }
.prompt-form button.secondary { color: var(--muted); background: transparent; border: 1px solid var(--border); }
</style>
