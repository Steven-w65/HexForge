<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import type { TemplateDefinition } from '../types'
import { createEditorBridge } from '../templateWindow/editorBridge'
import { errorMessage, type EditorActionName, type TemplateSnapshot } from '../templateWindow/protocol'
import { tauriBus } from '../templateWindow/tauriBus'
import AppDialog from './AppDialog.vue'
import TemplateEditor from './TemplateEditor.vue'
import { commandEnabled, commandUnavailableReason, type MenuCommand, type MenuState } from '../menu/commands'
import { validateTemplate } from '../template/model'
import { templateStatus } from '../template/status'
import { isModalOpen } from '../ui/modalFocus'

const empty: TemplateDefinition = { name: 'Untitled', defaultEndianness: 'little', fields: [] }
const model = ref<TemplateDefinition>(empty)
const snapshot = ref<TemplateSnapshot | null>(null)
const valid = ref(true)
const pendingUiEdits = ref(false)
const error = ref('')
const closePrompt = ref(false)
const checkpoint = ref<TemplateDefinition | null>(null)
const pendingAction = ref(false)
const isClosing = ref(false)
let checkpointRevision = -1
const disposers: Array<() => void> = []
let closing = false
let bridgeReady = false
let snapshotTimeout: ReturnType<typeof setTimeout> | null = null

const bridge = createEditorBridge(tauriBus, {
  onSnapshot(value) {
    if (snapshotTimeout) { clearTimeout(snapshotTimeout); snapshotTimeout = null }
    if (checkpoint.value === null || value.persistenceRevision !== checkpointRevision) {
      checkpoint.value = JSON.parse(JSON.stringify(value.checkpointTemplate)) as TemplateDefinition
      checkpointRevision = value.persistenceRevision
    }
    snapshot.value = value
    if (JSON.stringify(model.value) !== JSON.stringify(value.template)) model.value = value.template
    document.documentElement.dataset.theme = value.theme
  },
  onError(cause) { error.value = errorMessage(cause) },
})

const editorMenuState = computed<MenuState>(() => ({
  hasFile: snapshot.value?.fileSize !== null && snapshot.value?.fileSize !== undefined,
  hasBytes: BigInt(snapshot.value?.fileSize ?? '0') > 0n,
  singleByteSelected: false, editMode: false, canUndo: false, hasParsedResults: false,
  templateActive: Boolean(snapshot.value?.active), templateHasPath: Boolean(snapshot.value?.templateFilePath),
  templateHasFields: model.value.fields.length > 0,
  templateValid: valid.value && validateTemplate(model.value).length === 0,
  templateIssueCount: validateTemplate(model.value).length,
  operationBusy: Boolean(snapshot.value?.busy) || pendingAction.value || isClosing.value,
}))
const canSave = computed(() => commandEnabled('save-template', editorMenuState.value))
const canSaveAs = computed(() => commandEnabled('save-template-as', editorMenuState.value))
const canApply = computed(() => Boolean(snapshot.value?.canApply) && commandEnabled('apply-template', editorMenuState.value))
const reason = (command: MenuCommand): string | undefined => commandUnavailableReason(command, editorMenuState.value) ?? undefined
const changedSinceOpen = computed(() => pendingUiEdits.value || (checkpoint.value !== null && JSON.stringify(model.value) !== JSON.stringify(checkpoint.value)))
const stateLabel = computed(() => templateStatus({ source: !snapshot.value?.active ? 'none' : snapshot.value.templateFilePath ? 'file' : 'draft',
  dirty: Boolean(snapshot.value?.dirty), applied: Boolean(snapshot.value?.applied), needsRefresh: Boolean(snapshot.value?.resultsNeedRefresh),
  hasFile: snapshot.value?.fileSize !== null && snapshot.value?.fileSize !== undefined, hasDiagnostics: Boolean(snapshot.value?.hasDiagnostics) }))

async function updateModel(value: TemplateDefinition): Promise<void> {
  model.value = value
  try { await bridge.sendDraft(value, valid.value) } catch { /* bridge displays the error and keeps the draft */ }
}
async function updateValidity(value: boolean): Promise<void> {
  if (valid.value === value) return
  valid.value = value
  try { await bridge.sendDraft(model.value, value) } catch { /* draft remains local */ }
}
async function action(command: EditorActionName, range?: { start: bigint; end: bigint }): Promise<boolean> {
  const commands = { load: 'load-template', save: 'save-template', 'save-as': 'save-template-as', apply: 'apply-template', unload: 'unload-template' } as const
  const mapped = commands[command as keyof typeof commands]
  if (pendingAction.value || (mapped && !commandEnabled(mapped, { ...editorMenuState.value, operationBusy: Boolean(snapshot.value?.busy) }))) return false
  if (command === 'apply' && !snapshot.value?.canApply) return false
  pendingAction.value = true
  try {
    return await bridge.requestAction(command, range ? { start: range.start.toString(), end: range.end.toString() } : undefined)
  } catch (cause) { error.value = errorMessage(cause); return false }
  finally { pendingAction.value = false }
}

async function finishClose(command: 'close' | 'discard'): Promise<void> {
  if (!await action(command)) return
  await bridge.notifyClosed()
  await getCurrentWindow().destroy()
}

async function closeWindow(): Promise<void> {
  if (closing) return
  if (bridgeReady && snapshot.value && changedSinceOpen.value) { closePrompt.value = true; return }
  closing = true
  isClosing.value = true
  try {
    if (!bridgeReady || !snapshot.value) await getCurrentWindow().destroy()
    else await finishClose('close')
  } catch (cause) { error.value = errorMessage(cause) }
  finally { closing = false; isClosing.value = false }
}

async function resolveClose(choice: 'save' | 'save-as' | 'discard' | 'cancel'): Promise<void> {
  if (choice === 'cancel') { closePrompt.value = false; return }
  if (closing) return
  closing = true
  isClosing.value = true
  try {
    if (choice === 'discard') await finishClose('discard')
    // Main acknowledges the saved checkpoint. A draft that changed during Save
    // is still unsaved, so leave this confirmation/window open for a new choice.
    else if (await action(choice) && !changedSinceOpen.value) await finishClose('close')
  } catch (cause) { error.value = errorMessage(cause) }
  finally { closing = false; isClosing.value = false }
}

function onKeydown(event: KeyboardEvent): void {
  if (event.isComposing) return
  // Reserve both template Save keys even while a reply/dialog blocks actions.
  // Repeated presses must not fall through to the WebView's Save Page dialog.
  if (event.ctrlKey && !event.altKey && !event.metaKey && event.key.toLowerCase() === 's') event.preventDefault()
  if (event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey && event.key.toLowerCase() === 'w') {
    // Keep the close shortcut local to this window and reuse native-close draft
    // protection. Consume it through dialogs/in-flight actions without closing.
    event.preventDefault()
    if (!isModalOpen() && !pendingAction.value && !isClosing.value) void closeWindow()
    return
  }
  if (isModalOpen()) {
    if (event.ctrlKey && ['s', 'o', 'u', 'Enter'].includes(event.key.length === 1 ? event.key.toLowerCase() : event.key)) event.preventDefault()
    return // Includes confirmations inside FieldCard, not just this window's dialogs.
  }
  if (event.key === 'Escape' && error.value) { error.value = ''; event.preventDefault(); return }
  if (closePrompt.value || error.value || pendingAction.value || isClosing.value) return
  if (!event.ctrlKey || event.metaKey) return
  const command = event.altKey
    ? !event.shiftKey && event.key.toLowerCase() === 'o' ? 'load' : !event.shiftKey && event.key.toLowerCase() === 'u' ? 'unload' : null
    : event.key.toLowerCase() === 's' ? event.shiftKey ? 'save-as' : 'save' : !event.shiftKey && event.key === 'Enter' ? 'apply' : null
  if (command) {
    event.preventDefault()
    if (command === 'save' && !canSave.value) return
    if (command === 'save-as' && !canSaveAs.value) return
    if (command === 'apply' && !canApply.value) return
    if (command === 'unload' && !snapshot.value?.active) return
    void action(command)
  }
}

onMounted(async () => {
  window.addEventListener('keydown', onKeydown)
  try {
    await bridge.start()
    bridgeReady = true
    if (!snapshot.value) snapshotTimeout = setTimeout(() => { error.value = 'Template Editor could not connect to the main window.' }, 5000)
    const unlisten = await getCurrentWindow().onCloseRequested((event) => {
      event.preventDefault()
      void closeWindow()
    })
    disposers.push(unlisten)
  } catch (cause) { error.value = errorMessage(cause) }
})
onBeforeUnmount(() => {
  if (snapshotTimeout) clearTimeout(snapshotTimeout)
  window.removeEventListener('keydown', onKeydown)
  disposers.splice(0).forEach((dispose) => dispose())
  bridge.dispose()
})
</script>

<template>
  <main class="editor-window" data-testid="template-editor-window" @contextmenu.prevent>
    <header class="editor-header">
      <div><strong>Template Editor</strong><small :title="snapshot?.templateFilePath ?? undefined"><span class="context-name">{{ model.name || 'Untitled' }}<template v-if="snapshot?.templateFilePath"> · {{ snapshot.templateFilePath.split(/[\\/]/).pop() }}</template></span><span class="state-label" :data-testid="snapshot?.dirty ? 'template-modified' : 'template-status'"> · {{ stateLabel }}</span></small></div>
    </header>
    <TemplateEditor v-if="snapshot" :key="snapshot.workspaceRevision" :model-value="model" :results="snapshot.results"
      :menu-state="editorMenuState"
      :can-unload="Boolean(snapshot?.active)"
      :can-apply="canApply" :can-save="canSave" :can-save-as="canSaveAs" @update:model-value="updateModel" @validity="updateValidity" @pending-edits="pendingUiEdits = $event"
      @load="action('load')" @save="action('save')" @save-as="action('save-as')" @unload="action('unload')" @apply="action('apply')" @navigate="action('navigate', $event)" />
    <p v-else class="connecting" role="status">Connecting to HexForge…</p>
    <p v-if="snapshot?.notice" :key="snapshot.notice.id" class="action-notice" role="status">{{ snapshot.notice.text }}</p>
    <AppDialog :open="closePrompt" title="Unsaved template edits" message="Save changes made in this Template Editor before closing?" @close="resolveClose('cancel')">
      <div data-testid="editor-close-prompt" class="close-actions">
        <button type="button" data-action="editor-save" :disabled="!canSave" :title="reason('save-template')" @click="resolveClose('save')">Save</button>
        <button type="button" data-action="editor-save-as" :disabled="!canSaveAs" :title="reason('save-template-as')" @click="resolveClose('save-as')">Save As</button>
        <button type="button" data-action="editor-discard" @click="resolveClose('discard')">Don't Save</button>
        <button type="button" data-action="editor-cancel" autofocus @click="resolveClose('cancel')">Cancel</button>
      </div>
    </AppDialog>
    <AppDialog :open="Boolean(error)" title="Template action failed" :message="error" @close="error = ''" />
  </main>
</template>

<style scoped>
.editor-window { box-sizing: border-box; width: 100vw; height: 100vh; min-width: 0; display: flex; flex-direction: column; gap: 12px; padding: 12px; color: var(--text); background: var(--bg); overflow: hidden; }
.editor-header { display: flex; justify-content: space-between; align-items: center; gap: 10px; padding-bottom: 9px; border-bottom: 1px solid var(--border); }
.editor-header button { height: 27px; padding: 0 9px; color: var(--text); background: var(--button); border: 0; border-radius: 3px; font: inherit; font-size: var(--font-body); }
.editor-header button:hover { background: var(--hover); }
.editor-header div { display: grid; gap: 2px; min-width: 0; }
.editor-header strong { font-size: var(--font-heading); }
.editor-header small { display: flex; min-width: 0; color: var(--muted); font-size: var(--font-support); white-space: nowrap; }
.context-name { overflow: hidden; text-overflow: ellipsis; }
.state-label { flex: none; }
.state-label[data-testid='template-modified'] { color: var(--modified); }
.connecting { color: var(--muted); font-size: var(--font-body); }
.action-notice { flex: none; margin: 0; color: var(--text); font-size: var(--font-support); }
.close-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; margin-top: 14px; }
.close-actions button { width: auto; height: 27px; padding: 0 9px; color: var(--text); background: var(--button); border: 0; border-radius: 3px; font: inherit; font-size: var(--font-body); }
.close-actions button:hover:not(:disabled) { background: var(--hover); }
.close-actions button:disabled { opacity: .4; cursor: default; }
</style>
