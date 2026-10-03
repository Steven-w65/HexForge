<script setup lang="ts">
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { TauriEvent } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { confirm, message, open, save } from '@tauri-apps/plugin-dialog'
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import AppShell from './components/AppShell.vue'
import { backend } from './api/backend'
import { useHexSession } from './composables/useHexSession'
import { handleCloseRequest, useHotkeys } from './composables/useHotkeys'
import { useTheme } from './composables/useTheme'
import type { BytesPerRow } from './hex/layout'
import { DEFAULT_MINIMAP_SETTINGS, type MinimapSettings } from './hex/minimapGeometry'
import { commandEnabled, type MenuCommand, type MenuState } from './menu/commands'
import type { TemplateDefinition } from './types'
import { flattenResultLeaves, hasResultDiagnostics, validateTemplate } from './template/model'
import { normalizeSelection, type ByteSelection } from './hex/selection'
import { parseHexBytes } from './hex/input'
import { createMainBridge } from './templateWindow/mainBridge'
import { tauriBus } from './templateWindow/tauriBus'
import { templateWindowManager } from './templateWindow/windowManager'
import type { EditorAction } from './templateWindow/protocol'
import { createListenerScope } from './startup/listenerScope'
import { startupTimings } from './startup/timings'

type PromptKind = 'edit'
startupTimings.mark('main-setup')
const session = useHexSession()
const bytesPerRow = ref<BytesPerRow>(16)
const minimapSettings = ref<MinimapSettings>({ ...DEFAULT_MINIMAP_SETTINGS })
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
const searchOpen = ref(false)
const searchValue = ref('')
const searchError = ref('')
const searchFocusKey = ref(0)
const currentSearchPattern = computed(() => {
  if (!session.searchQuery.value) return false
  try { return parseHexBytes(searchValue.value).join(' ') === parseHexBytes(session.searchQuery.value).join(' ') }
  catch { return false }
})
const canNavigateSearch = computed(() => currentSearchPattern.value && session.matches.value.length > 0 && !menuState.value.operationBusy)
const gotoOpen = ref(false)
const gotoValue = ref('')
const gotoError = ref('')
const gotoFocusKey = ref(0)
const templateRange = ref<ByteSelection | null>(null)
// A template highlight belongs to its navigation action, not to every later
// byte selection. Scroll-only changes never touch selection or edited bytes.
watch(() => session.selection.value, () => { templateRange.value = null }, { flush: 'sync' })
const templateValid = ref(true)
const completionNotice = ref<{ id: number; text: string } | null>(null)
let noticeId = 0
let noticeTimer: ReturnType<typeof setTimeout> | undefined
function clearCompletion(): void { if (noticeTimer) clearTimeout(noticeTimer); completionNotice.value = null }
function notifyCompletion(text: string): void {
  clearCompletion()
  const id = ++noticeId
  completionNotice.value = { id, text }
  noticeTimer = setTimeout(() => { if (completionNotice.value?.id === id) completionNotice.value = null }, 5000)
}
watch(() => session.error.value, error => { if (error) clearCompletion() })
onBeforeUnmount(clearCompletion)
const closeGuard = { confirming: false }
const binaryWorkflowBusy = ref(false)
const mainClosing = ref(false)
const nativeCloseProtected = ref(false)
type BinaryChoice = 'save' | 'discard' | 'cancel'
const binaryPrompt = ref<{ message: string } | null>(null)
let resolveBinaryChoice: ((choice: BinaryChoice) => void) | null = null
function chooseBinaryAction(choice: BinaryChoice): void {
  const resolve = resolveBinaryChoice
  resolveBinaryChoice = null
  binaryPrompt.value = null
  resolve?.(choice)
}
onBeforeUnmount(() => chooseBinaryAction('cancel'))
const startupListeners = createListenerScope({ disposeOnError: false })
let disposed = false

function templateSnapshot(value: TemplateDefinition): string { return JSON.stringify(value) }
function copyTemplate(value: TemplateDefinition): TemplateDefinition { return JSON.parse(templateSnapshot(value)) as TemplateDefinition }
const templateBaseline = ref(templateSnapshot(session.template.value))
// Rejected editor inputs remain local to the field controls, but their invalid
// draft flag still protects them from silent loss on load, unload, or app exit.
const templateDirty = computed(() => !templateValid.value || templateSnapshot(session.template.value) !== templateBaseline.value)
const templateActive = ref(false)
const templateFilePath = ref<string | null>(null)
const templatePersistenceRevision = ref(0)
const templateWorkspaceRevision = ref(0)
const templateWorkflowBusy = ref(false)
let editorCheckpoint: { template: TemplateDefinition; baseline: string; active: boolean; path: string | null } | null = null
const templateSource = computed(() => !templateActive.value ? 'none' : templateFilePath.value ? 'file' : 'draft')
const templateDisplayName = computed(() => templateFilePath.value ? filenameFromPath(templateFilePath.value) : session.template.value.name)

function filenameFromPath(path: string): string { return path.split(/[\\/]/).pop() || path }

function checkpointEditor(template = session.template.value): void {
  if (!bridge.isReady()) return
  editorCheckpoint = { template: copyTemplate(template), baseline: templateBaseline.value, active: templateActive.value, path: templateFilePath.value }
}

function discardEditorChanges(): void {
  if (!editorCheckpoint) throw { code: 'operation_failed', message: 'The Template Editor has no draft to restore.' }
  const previous = editorCheckpoint
  session.updateTemplate(copyTemplate(previous.template))
  templateBaseline.value = previous.baseline
  templateActive.value = previous.active
  templateFilePath.value = previous.path
  templateValid.value = true
  clearTemplateHighlight()
  templatePersistenceRevision.value += 1
  templateWorkspaceRevision.value += 1
  checkpointEditor()
}

async function withTemplateWorkflow<T>(operation: () => Promise<T>): Promise<T | false> {
  if (templateWorkflowBusy.value) return false
  templateWorkflowBusy.value = true
  try { return await operation() }
  finally { templateWorkflowBusy.value = false }
}

async function reportFailure(operation: () => Promise<unknown>): Promise<boolean> {
  try { await operation(); return true }
  catch (error) { session.presentError(error); return false }
}

async function nativeCall<T>(operation: () => Promise<T>): Promise<T | undefined> {
  try { return await operation() }
  catch (error) { session.presentError(error); return undefined }
}

async function withBinaryWorkflow(operation: () => Promise<unknown>): Promise<void> {
  if (!nativeCloseProtected.value || binaryWorkflowBusy.value || mainClosing.value) return
  binaryWorkflowBusy.value = true
  try { await operation() }
  finally { binaryWorkflowBusy.value = false }
}

/** null cancels; true approves discard; false continues after a successful save. */
async function confirmBinaryContinuation(reason: string, forClose = false): Promise<boolean | null> {
  const choice = await new Promise<BinaryChoice>(resolve => {
    resolveBinaryChoice = resolve
    binaryPrompt.value = { message: `Save unsaved byte edits to "${session.file.value?.name ?? 'this binary'}" before ${reason}? The original binary will not be overwritten.` }
  })
  if (choice === 'cancel') return null
  if (choice === 'discard') return true
  return await chooseSaveAs(forClose) ? false : null
}

async function confirmExitChanges(fileDirty: boolean): Promise<boolean> {
  if (fileDirty && await confirmBinaryContinuation('exiting HexForge', true) === null) return false
  // The companion can edit while a native save picker is open. Re-read its
  // latest draft before making the independent template-discard decision.
  await bridge.flush()
  if (!templateDirty.value) return true
  return await nativeCall(() => confirm('Unsaved template changes will be lost. Exit HexForge?', { title: 'HexForge', kind: 'warning' })) === true
}

async function openPathCore(path: string): Promise<void> {
  let discard = false
  if (session.file.value?.dirty) {
    const decision = await confirmBinaryContinuation('opening another binary')
    if (decision === null) return
    discard = decision
  }
  if (await reportFailure(() => session.openFile(path, discard))) clearTemplateHighlight()
}
function openPath(path: string): Promise<void> { return withBinaryWorkflow(() => openPathCore(path)) }

async function chooseFile(): Promise<void> {
  await withBinaryWorkflow(async () => {
    const selected = await nativeCall(() => open({ multiple: false, directory: false, title: 'Open binary file' }))
    if (typeof selected === 'string') await openPathCore(selected)
  })
}

async function chooseSaveAs(forClose = false): Promise<boolean> {
  clearCompletion()
  const path = await nativeCall(() => save({ title: 'Save binary as', defaultPath: session.file.value ? `${session.file.value.name}.copy` : undefined }))
  if (!path) return false
  const saved = await reportFailure(() => forClose ? session.saveAsForClose(path) : session.saveAs(path))
  if (saved) notifyCompletion(`Copy saved as ${filenameFromPath(path)}.`)
  return saved
}

async function closeCurrentFile(): Promise<void> {
  await withBinaryWorkflow(async () => {
    if (!session.file.value) return
    let discard = false
    if (session.file.value.dirty) {
      const decision = await confirmBinaryContinuation('closing this binary')
      if (decision === null) return
      discard = decision
    }
    await reportFailure(() => session.closeFile(discard))
    if (!session.file.value) clearTemplateHighlight()
  })
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
    templateFilePath.value = path
    templatePersistenceRevision.value += 1
    templateWorkspaceRevision.value += 1
    checkpointEditor()
  })
}

function isErrorCode(error: unknown, code: string): boolean {
  return typeof error === 'object' && error !== null && 'code' in error && error.code === code
}

async function chooseTemplateSave(flushDraft = true): Promise<boolean> {
  clearCompletion()
  if (flushDraft) await bridge.flush()
  if (!templateActive.value || !templateFilePath.value) return false
  if (!templateValid.value) { throw { code: 'invalid_template', message: 'Correct the highlighted template field before saving.' } }
  const definition = copyTemplate(session.template.value)
  const savedSnapshot = templateSnapshot(definition)
  try { await session.saveTemplate(false, definition) }
  catch (error) {
    if (!isErrorCode(error, 'external_modification')) throw error
    session.clearError()
    const approved = await inTemplateDialog(!flushDraft, () => confirm('The file has been modified by another program. Overwrite?', { title: 'HexForge', kind: 'warning' }))
    if (!approved) return false
    await session.saveTemplate(true, definition)
  }
  templateBaseline.value = savedSnapshot
  templatePersistenceRevision.value += 1
  checkpointEditor(definition)
  notifyCompletion(`Template saved: ${filenameFromPath(templateFilePath.value)}.`)
  return true
}

async function chooseTemplateSaveAs(flushDraft = true): Promise<boolean> {
  clearCompletion()
  if (flushDraft) await bridge.flush()
  if (!templateActive.value) return false
  if (!templateValid.value) { throw { code: 'invalid_template', message: 'Correct the highlighted template field before saving.' } }
  return inTemplateDialog(!flushDraft, async () => {
    const path = await save({ title: 'Save parsing template', defaultPath: `${session.template.value.name || 'template'}.json`, filters: [{ name: 'JSON template', extensions: ['json'] }] })
    if (!path) return false
    const definition = copyTemplate(session.template.value)
    const savedSnapshot = templateSnapshot(definition)
    try { await session.saveTemplateAs(path, false, definition) }
    catch (error) {
      if (!isErrorCode(error, 'destination_exists')) throw error
      session.clearError()
      if (!await confirm('File already exists. Overwrite?', { title: 'HexForge', kind: 'warning' })) return false
      await session.saveTemplateAs(path, true, definition)
    }
    templateBaseline.value = savedSnapshot
    templateFilePath.value = path
    templatePersistenceRevision.value += 1
    checkpointEditor(definition)
    notifyCompletion(`Template saved as ${filenameFromPath(path)}.`)
    return true
  })
}

async function chooseCsvExport(): Promise<void> {
  clearCompletion()
  const path = await nativeCall(() => save({ title: 'Export parsed results', defaultPath: `${session.template.value.name || 'results'}.csv`, filters: [{ name: 'CSV', extensions: ['csv'] }] }))
  if (path && await reportFailure(() => session.exportCsv(path))) notifyCompletion(`CSV exported: ${filenameFromPath(path)}.`)
}

function showPrompt(kind: PromptKind, title: string, value = ''): void { popup.value = { kind, title, value } }
function closePopup(): void { chooseBinaryAction('cancel'); popup.value = null; session.clearError() }
function closeTopLayer(): void {
  if (popup.value !== null || session.error.value !== null) closePopup()
  else if (gotoOpen.value) gotoOpen.value = false
  else if (searchOpen.value) searchOpen.value = false
}

function openGoto(): void {
  if (!gotoOpen.value) { gotoValue.value = ''; gotoError.value = '' }
  searchOpen.value = false
  gotoOpen.value = true
  gotoFocusKey.value += 1
}

function submitGoto(): void {
  gotoError.value = ''
  try {
    session.goTo(gotoValue.value)
    gotoOpen.value = false
  } catch (cause) {
    gotoError.value = typeof cause === 'object' && cause !== null && 'message' in cause ? String(cause.message) : 'The offset is invalid.'
    session.clearError()
  }
}

function openSearch(): void {
  if (!searchOpen.value) {
    searchValue.value = session.searchQuery.value
    searchError.value = ''
  }
  gotoOpen.value = false
  searchOpen.value = true
  searchFocusKey.value += 1
}

async function submitSearch(force = false, direction: 1 | -1 = 1): Promise<void> {
  if (menuState.value.operationBusy) return
  searchError.value = ''
  if (!force && canNavigateSearch.value) { session.navigateSearch(direction); return }
  try { await session.search(searchValue.value) }
  catch (cause) {
    searchError.value = cause instanceof Error ? cause.message :
      typeof cause === 'object' && cause !== null && 'message' in cause ? String(cause.message) : 'Search could not be completed.'
  }
}

function clearSearch(): void {
  if (session.busy.search) return
  session.clearSearch()
  searchValue.value = ''
  searchError.value = ''
}

async function submitPrompt(): Promise<void> {
  const active = popup.value
  if (!active) return
  try {
    await session.editSelectedByte(active.value)
    popup.value = null
  } catch { /* validation is rendered in the in-app dialog */ }
}

function beginEdit(offset: bigint): void {
  if (binaryWorkflowBusy.value || mainClosing.value) return
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
  clearCompletion()
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
  await session.unloadTemplateFile()
  session.unloadTemplate()
  templateBaseline.value = templateSnapshot(session.template.value)
  templateActive.value = false
  templateFilePath.value = null
  templatePersistenceRevision.value += 1
  templateWorkspaceRevision.value += 1
  checkpointEditor()
  clearTemplateHighlight()
  templateValid.value = true
}
async function applyValidTemplate(flushDraft = true): Promise<void> {
  clearCompletion()
  if (flushDraft) await bridge.flush()
  if (!templateValid.value) { throw { code: 'invalid_template', message: 'Correct the highlighted template field before applying.' } }
  await session.applyTemplate()
  // A newer draft/content change can make an in-flight parse irrelevant.
  if (session.templateApplied.value) {
    const count = flattenResultLeaves(session.results.value).length
    notifyCompletion(hasResultDiagnostics(session.results.value) ? 'Parsing completed with field errors. Inspect parsed results.' : `Parsed ${count} ${count === 1 ? 'field' : 'fields'}.`)
  }
}
function navigateTemplate(range: { start: bigint; end: bigint }): void {
  session.navigate(range)
  templateRange.value = normalizeSelection(range.start, range.end)
}
function selectBytes(value: ByteSelection): void { templateRange.value = null; session.selection.value = value }
function clearSelection(): void { templateRange.value = null; session.clearSelection() }

const activeBusy = computed(() => {
  const labels: Array<[keyof typeof session.busy, string]> = [
    ['open', 'Opening file'], ['close', 'Closing file'], ['search', 'Searching bytes'], ['parse', 'Parsing template'], ['save', 'Saving copy'],
    ['export', 'Exporting CSV'], ['template', 'Working with template'], ['edit', 'Applying edit'], ['undo', 'Undoing edit'], ['page', 'Loading bytes'],
  ]
  const operation = session.activity.value?.operation
  return operation ? labels.find(([name]) => name === operation)?.[1] ?? '' : ''
})
const progressText = computed(() => {
  const progress = session.activity.value?.progress
  if (!progress) return ''
  // Reference counts/conditions make the final work size data-dependent.
  // Display actual nested work, not a percentage of top-level definitions.
  if (progress.phase === 'parse' && progress.total === '0') return `${progress.processed} nodes processed`
  return `${progress.processed} / ${progress.total}`
})

const menuState = computed<MenuState>(() => ({
  hasFile: session.file.value !== null,
  hasBytes: BigInt(session.file.value?.size ?? '0') > 0n,
  singleByteSelected: session.selection.value?.count === 1n,
  editMode: session.editMode.value,
  canUndo: session.canUndo.value,
  templateValid: templateValid.value,
  templateIssueCount: validateTemplate(session.template.value).length,
  templateActive: templateActive.value,
  templateHasPath: templateFilePath.value !== null,
  templateHasFields: session.template.value.fields.length > 0,
  hasParsedResults: flattenResultLeaves(session.results.value).length > 0 && !hasResultDiagnostics(session.results.value),
  hasSearchMatches: currentSearchPattern.value && session.matches.value.length > 0,
  operationBusy: !nativeCloseProtected.value || session.activity.value !== null || templateWorkflowBusy.value || binaryWorkflowBusy.value || mainClosing.value,
}))

const bridge = createMainBridge(tauriBus, {
  snapshot: () => ({
    template: session.template.value, results: session.results.value, fileSize: session.file.value?.size ?? null,
    theme: theme.value.value, dirty: templateDirty.value, canApply: commandEnabled('apply-template', menuState.value), active: templateActive.value,
    busy: menuState.value.operationBusy,
    applied: session.templateApplied.value, resultsNeedRefresh: session.resultsNeedRefresh.value,
    hasDiagnostics: hasResultDiagnostics(session.results.value), notice: completionNotice.value,
    templateFilePath: templateFilePath.value, persistenceRevision: templatePersistenceRevision.value, workspaceRevision: templateWorkspaceRevision.value,
    checkpointTemplate: editorCheckpoint?.template ?? session.template.value,
  }),
  onReady: checkpointEditor,
  onDraft: (draft) => {
    if (templateSnapshot(session.template.value) !== templateSnapshot(draft.template)) updateTemplate(draft.template)
    templateValid.value = draft.valid
  },
  onAction: async (action: EditorAction) => {
    const commands = { load: 'load-template', save: 'save-template', 'save-as': 'save-template-as', apply: 'apply-template', unload: 'unload-template' } as const
    const command = commands[action.command as keyof typeof commands]
    if (command && !commandEnabled(command, menuState.value)) return false
    switch (action.command) {
      case 'load': return await withTemplateWorkflow(() => chooseTemplateLoad(false))
      case 'save': return await withTemplateWorkflow(() => chooseTemplateSave(false))
      case 'save-as': return await withTemplateWorkflow(() => chooseTemplateSaveAs(false))
      case 'apply': return await withTemplateWorkflow(() => applyValidTemplate(false))
      case 'unload': return await withTemplateWorkflow(() => unloadTemplate(false))
      case 'navigate': if (action.range) navigateTemplate({ start: BigInt(action.range.start), end: BigInt(action.range.end) }); break
      case 'discard': if (templateWorkflowBusy.value) return false; discardEditorChanges(); break
      case 'close': if (templateWorkflowBusy.value) return false; break // The accepted draft remains in the authoritative main session.
    }
  },
  onError: (error) => session.presentError(error),
  onClosed: () => { editorCheckpoint = null; templateWindowManager.forget() },
})

watch(() => [session.template.value, session.results.value, session.file.value?.size, theme.value.value,
  templateDirty.value, templateActive.value, templateValid.value, templateFilePath.value, templatePersistenceRevision.value, menuState.value.operationBusy,
  session.templateApplied.value, session.resultsNeedRefresh.value, completionNotice.value],
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
    case 'save-as': void withBinaryWorkflow(() => chooseSaveAs()); break
    case 'export': void chooseCsvExport(); break
    case 'exit': void exitApplication(); break
    case 'edit-selected': {
      const selected = session.selection.value
      if (selected) beginEdit(selected.start)
      break
    }
    case 'toggle-edit': session.editMode.value = !session.editMode.value; break
    case 'undo': void reportFailure(session.undo); break
    case 'goto': openGoto(); break
    case 'search': openSearch(); break
    case 'search-next': session.navigateSearch(1); break
    case 'search-previous': session.navigateSearch(-1); break
    case 'template-editor': void openTemplateEditor(); break
    case 'apply-template': void reportFailure(() => withTemplateWorkflow(applyValidTemplate)); break
    case 'load-template': void reportFailure(() => withTemplateWorkflow(chooseTemplateLoad)); break
    case 'unload-template': void reportFailure(() => withTemplateWorkflow(unloadTemplate)); break
    case 'save-template': void reportFailure(() => withTemplateWorkflow(chooseTemplateSave)); break
    case 'save-template-as': void reportFailure(() => withTemplateWorkflow(chooseTemplateSaveAs)); break
    case 'theme-toggle': theme.toggle(); break
    case 'row-16': bytesPerRow.value = 16; break
    case 'row-32': bytesPerRow.value = 32; break
    case 'minimap-toggle': minimapSettings.value = { ...minimapSettings.value, enabled: !minimapSettings.value.enabled }; break
    case 'minimap-fit': minimapSettings.value = { ...minimapSettings.value, mode: 'fit' }; break
    case 'minimap-proportional': minimapSettings.value = { ...minimapSettings.value, mode: 'proportional' }; break
    case 'minimap-characters': minimapSettings.value = { ...minimapSettings.value, renderCharacters: true }; break
    case 'minimap-blocks': minimapSettings.value = { ...minimapSettings.value, renderCharacters: false }; break
    case 'minimap-scale-1': minimapSettings.value = { ...minimapSettings.value, scale: 1 }; break
    case 'minimap-scale-2': minimapSettings.value = { ...minimapSettings.value, scale: 2 }; break
    case 'minimap-scale-3': minimapSettings.value = { ...minimapSettings.value, scale: 3 }; break
    case 'toggle-right-panel': rightCollapsed.value = !rightCollapsed.value; storePanelPreference(RIGHT_PANEL_KEY, rightCollapsed.value); break
  }
}

onMounted(async () => {
  startupTimings.mark('main-mounted')
  startupListeners.own(useHotkeys({
    invoke: executeCommand, isEnabled: (command) => commandEnabled(command, menuState.value),
    isPopupOpen: () => popup.value !== null || session.error.value !== null || gotoOpen.value || searchOpen.value,
    closePopup: closeTopLayer, clearSelection,
  }))
  try {
    startupTimings.mark('listeners-start')
    await startupListeners.register([
      async () => { await bridge.start(); startupTimings.mark('bridge-ready'); return bridge.dispose },
      () => getCurrentWindow().onCloseRequested(async (event) => {
      if (binaryWorkflowBusy.value || closeGuard.confirming) { event.preventDefault(); return }
      mainClosing.value = true
      let fileDirty = false
      let templateDraftDirty = false
      try {
        await handleCloseRequest(event, async () => {
          await bridge.flush()
          const state = await session.prepareClose()
          fileDirty = state.dirty
          templateDraftDirty = templateDirty.value
          return { dirty: fileDirty || templateDraftDirty }
        }, () => confirmExitChanges(fileDirty), closeGuard, session.releaseCloseBarrier,
        () => templateWindowManager.destroy())
      }
      catch (error) { session.presentError(error) }
      finally { mainClosing.value = false }
      }).then(unlisten => { if (!disposed) nativeCloseProtected.value = true; startupTimings.mark('close-listener-ready'); return unlisten }),
      // We only consume drops. The convenience helper also serially registers
      // enter/over/leave events, adding three unnecessary startup IPC trips.
      () => getCurrentWebview().listen<{ paths: string[] }>(TauriEvent.DRAG_DROP, (event) => {
        if (event.payload.paths.length === 1) void openPath(event.payload.paths[0]!)
        else void nativeCall(() => message('Drop exactly one file at a time.', { title: 'HexForge', kind: 'warning' }))
      }).then(unlisten => { startupTimings.mark('drop-listener-ready'); return unlisten }),
    ])
    if (disposed) return
    startupTimings.mark('listeners-ready')
    // CI sets a process-local flag; this native acknowledgment proves that
    // the bundled Vue frontend and IPC both started in the portable EXE.
    try { await backend.frontendReady(await startupTimings.completeReport(backend.startupClock)) } catch { /* Startup probe is optional in normal runs. */ }
  } catch (error) {
    if (disposed) return
    session.presentError(error)
    await nativeCall(() => message(error instanceof Error ? error.message : 'Desktop listeners could not be registered.', { title: 'HexForge', kind: 'error' }))
  }
})

onBeforeUnmount(() => { disposed = true; bridge.dispose(); startupListeners.dispose() })
watch(() => session.sourceIdentity.value, () => {
  searchOpen.value = false; searchValue.value = ''; searchError.value = ''
  gotoOpen.value = false; gotoValue.value = ''; gotoError.value = ''
})
</script>

<template>
  <AppShell
    data-testid="hexforge-app" :file="session.file.value" :source-identity="session.sourceIdentity.value" :page="session.page.value" :selection="session.selection.value"
    :template="session.template.value" :results="session.results.value" :matches="session.matches.value" :modified-overview="session.modifiedOverview.value"
    :template-source="templateSource" :template-display-name="templateDisplayName" :template-applied="session.templateApplied.value"
    :template-dirty="templateDirty" :results-need-refresh="session.resultsNeedRefresh.value" :completion-notice="completionNotice"
    :match-length="session.searchMatchLength.value" :search-truncated="session.searchTruncated.value" :template-range="templateRange"
    :search-open="searchOpen" :search-value="searchValue" :search-query="session.searchQuery.value" :search-count="session.matches.value.length"
    :search-busy="session.busy.search" :search-error="searchError" :search-focus-key="searchFocusKey"
    :search-index="session.searchMatchIndex.value" :search-can-navigate="canNavigateSearch"
    :search-needs-update="Boolean(session.searchQuery.value) && !currentSearchPattern"
    :goto-open="gotoOpen" :goto-value="gotoValue" :goto-error="gotoError" :goto-focus-key="gotoFocusKey"
    :busy-label="activeBusy" :progress-text="progressText"
    :template-valid="templateValid"
    :menu-state="menuState" :theme="theme.value.value" :right-collapsed="rightCollapsed" :minimap-settings="minimapSettings" :edit-delta="session.lastEditDelta.value"
    :bytes-per-row="bytesPerRow" :edit-mode="session.editMode.value" :endianness="session.template.value.defaultEndianness"
    :navigation-offset="session.viewportOffset.value" :dialog-open="binaryPrompt !== null || popup !== null || session.error.value !== null"
    :dialog-title="binaryPrompt ? 'Unsaved byte edits' : popup?.title ?? (session.error.value ? 'Operation failed' : '')" :dialog-message="binaryPrompt?.message ?? session.error.value?.message ?? ''"
    @command="executeCommand" @update:bytes-per-row="bytesPerRow = $event" @navigate="navigateTemplate"
    @request-page="reportFailure(() => session.requestPage($event.offset, $event.length, $event.generation))" @select="selectBytes"
    @edit-request="beginEdit" @viewport-offset="session.viewportOffset.value = $event" @minimap-error="session.presentError($event)" @close-dialog="closePopup"
    @update:search-value="searchValue = $event" @submit-search="submitSearch" @clear-search="clearSearch" @close-search="searchOpen = false"
    @navigate-search="session.navigateSearch($event)"
    @update:goto-value="gotoValue = $event" @submit-goto="submitGoto" @close-goto="gotoOpen = false"
  >
    <template #dialog>
      <div v-if="binaryPrompt" class="prompt-actions binary-actions">
        <button type="button" data-action="binary-save-continue" @click="chooseBinaryAction('save')">Save Copy and Continue</button>
        <button type="button" data-action="binary-discard" @click="chooseBinaryAction('discard')">Discard</button>
        <button type="button" data-action="binary-cancel" autofocus @click="chooseBinaryAction('cancel')">Cancel</button>
      </div>
      <form v-else-if="popup" class="prompt-form" @submit.prevent="submitPrompt">
        <label for="prompt-value">Hex byte value</label>
        <input id="prompt-value" v-model="popup.value" autofocus placeholder="FF">
        <small>Enter one hexadecimal byte from 00 to FF.</small>
        <div class="prompt-actions"><button type="button" class="secondary" @click="closePopup">Cancel</button><button type="submit">Apply</button></div>
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
.prompt-actions button { height: 28px; padding: 0 14px; color: var(--text); background: var(--button); border: 0; border-radius: 4px; font: inherit; }
.prompt-actions button:hover { background: var(--hover); }
.prompt-actions button.secondary { color: var(--muted); background: transparent; border: 1px solid var(--border); }
.binary-actions { margin-top: 15px; flex-wrap: wrap; }
</style>
