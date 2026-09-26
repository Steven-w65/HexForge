<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import type { TemplateDefinition } from '../types'
import { createEditorBridge } from '../templateWindow/editorBridge'
import { errorMessage, type EditorActionName, type TemplateSnapshot } from '../templateWindow/protocol'
import { tauriBus } from '../templateWindow/tauriBus'
import AppDialog from './AppDialog.vue'
import TemplateEditor from './TemplateEditor.vue'

const empty: TemplateDefinition = { version: 1, name: 'Untitled', defaultEndianness: 'little', fields: [] }
const model = ref<TemplateDefinition>(empty)
const snapshot = ref<TemplateSnapshot | null>(null)
const valid = ref(true)
const error = ref('')
const closePrompt = ref(false)
const checkpoint = ref<TemplateDefinition | null>(null)
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

const fileSize = computed(() => snapshot.value?.fileSize === null || snapshot.value?.fileSize === undefined ? null : BigInt(snapshot.value.fileSize))
const canSave = computed(() => Boolean(snapshot.value?.active && snapshot.value.templateFilePath))
const canSaveAs = computed(() => Boolean(snapshot.value?.active))
const changedSinceOpen = computed(() => checkpoint.value !== null && JSON.stringify(model.value) !== JSON.stringify(checkpoint.value))

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
  try {
    return await bridge.requestAction(command, range ? { start: range.start.toString(), end: range.end.toString() } : undefined)
  } catch (cause) { error.value = errorMessage(cause); return false }
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
  try {
    if (!bridgeReady || !snapshot.value) await getCurrentWindow().destroy()
    else await finishClose('close')
  } catch (cause) { error.value = errorMessage(cause) }
  finally { closing = false }
}

async function resolveClose(choice: 'save' | 'save-as' | 'discard' | 'cancel'): Promise<void> {
  if (choice === 'cancel') { closePrompt.value = false; return }
  if (closing) return
  closing = true
  try {
    if (choice === 'discard') await finishClose('discard')
    else if (await action(choice)) await finishClose('close')
  } catch (cause) { error.value = errorMessage(cause) }
  finally { closing = false }
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape' && error.value) { error.value = ''; event.preventDefault(); return }
  if (!event.ctrlKey || event.metaKey) return
  const command = event.altKey
    ? !event.shiftKey && event.key.toLowerCase() === 'o' ? 'load' : !event.shiftKey && event.key.toLowerCase() === 'u' ? 'unload' : null
    : event.key.toLowerCase() === 's' ? event.shiftKey ? 'save-as' : 'save' : !event.shiftKey && event.key === 'Enter' ? 'apply' : null
  if (command) {
    event.preventDefault()
    if (command === 'save' && !canSave.value) return
    if (command === 'save-as' && !canSaveAs.value) return
    if (command === 'apply' && (!valid.value || !snapshot.value?.canApply)) return
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
  <main class="editor-window" data-testid="template-editor-window">
    <header class="editor-header">
      <div><strong>Template Editor</strong><small>{{ model.name || 'Untitled' }}<span v-if="snapshot?.dirty" data-testid="template-modified"> · Modified</span></small></div>
      <button type="button" data-action="close-template-editor" aria-label="Close Template Editor" @click="closeWindow">×</button>
    </header>
    <TemplateEditor v-if="snapshot" :model-value="model" :file-size="fileSize" :results="snapshot.results"
      :can-unload="Boolean(snapshot?.active)"
      :can-apply="Boolean(snapshot?.canApply) && valid" :can-save="canSave" :can-save-as="canSaveAs" @update:model-value="updateModel" @validity="updateValidity"
      @load="action('load')" @save="action('save')" @save-as="action('save-as')" @unload="action('unload')" @apply="action('apply')" @navigate="action('navigate', $event)" />
    <p v-else class="connecting" role="status">Connecting to HexForge…</p>
    <AppDialog :open="closePrompt" title="Unsaved template edits" message="Save changes made in this Template Editor before closing?" @close="resolveClose('cancel')">
      <div data-testid="editor-close-prompt" class="close-actions">
        <button type="button" data-action="editor-save" :disabled="!canSave" @click="resolveClose('save')">Save</button>
        <button type="button" data-action="editor-save-as" :disabled="!canSaveAs" @click="resolveClose('save-as')">Save As</button>
        <button type="button" data-action="editor-discard" @click="resolveClose('discard')">Don't Save</button>
        <button type="button" data-action="editor-cancel" @click="resolveClose('cancel')">Cancel</button>
      </div>
    </AppDialog>
    <AppDialog :open="Boolean(error)" title="Template action failed" :message="error" @close="error = ''" />
  </main>
</template>

<style scoped>
.editor-window { box-sizing: border-box; width: 100vw; height: 100vh; min-width: 0; display: flex; flex-direction: column; gap: 12px; padding: 12px; color: var(--text); background: var(--bg); overflow: hidden; }
.editor-header { display: flex; justify-content: space-between; align-items: center; gap: 10px; padding-bottom: 9px; border-bottom: 1px solid var(--border); }
.editor-header div { display: grid; gap: 2px; min-width: 0; }
.editor-header strong { font-size: var(--font-heading); }
.editor-header small { overflow: hidden; color: var(--muted); font-size: var(--font-support); text-overflow: ellipsis; white-space: nowrap; }
.editor-header small span { color: var(--modified); }
.editor-header button { width: 26px; height: 26px; color: var(--muted); background: transparent; border: 0; border-radius: 4px; font: inherit; font-size: 18px; }
.editor-header button:hover { color: var(--text); background: var(--hover); }
.connecting { color: var(--muted); font-size: var(--font-body); }
.close-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; margin-top: 14px; }
.close-actions button { width: auto; height: 28px; padding: 0 9px; color: var(--text); background: var(--button); font-size: var(--font-body); }
.close-actions button:disabled { opacity: .4; cursor: default; }
</style>
