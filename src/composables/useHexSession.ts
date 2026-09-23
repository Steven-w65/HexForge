import { computed, reactive, ref, type ComputedRef, type Ref } from 'vue'
import { backend as defaultBackend } from '../api/backend'
import { parseHexBytes, parseOffset, parseSingleByte } from '../hex/input'
import { normalizeSelection, type ByteSelection } from '../hex/selection'
import type {
  AppError, DirtyState, FileInfo, OperationProgress, PageResponse, ParsedField,
  SaveResponse, SearchResponse, TemplateDefinition, UndoResponse, ViewportPage,
} from '../types'

type ProgressHandler = (value: OperationProgress) => void

export interface HexBackend {
  openFile(path: string, discardUnsaved?: boolean): Promise<FileInfo>
  closeFile(discardUnsaved?: boolean): Promise<void>
  getFileInfo(): Promise<FileInfo>
  readPage(offset: bigint, length: number): Promise<PageResponse>
  editByte(offset: bigint, value: number): Promise<DirtyState>
  undoEdit(): Promise<UndoResponse>
  getDirtyState(): Promise<DirtyState>
  saveAs(path: string, onProgress: ProgressHandler): Promise<SaveResponse>
  searchBytes(pattern: string, onProgress: ProgressHandler): Promise<SearchResponse>
  applyTemplate(template: TemplateDefinition, onProgress: ProgressHandler): Promise<ParsedField[]>
  loadTemplate(path: string): Promise<TemplateDefinition>
  saveTemplate(path: string, template: TemplateDefinition): Promise<void>
  exportResultsCsv(path: string, template: TemplateDefinition, onProgress: ProgressHandler): Promise<void>
}

export type BusyOperation = 'open' | 'close' | 'page' | 'search' | 'parse' | 'edit' | 'undo' | 'save' | 'template' | 'export'
export interface OperationActivity { operation: BusyOperation; progress: OperationProgress | null }

const EMPTY_TEMPLATE: TemplateDefinition = { version: 1, name: 'Untitled', defaultEndianness: 'little', fields: [] }

function friendlyError(value: unknown): AppError {
  if (typeof value === 'object' && value !== null && 'code' in value && typeof value.code === 'string' &&
      'message' in value && typeof value.message === 'string') return value as AppError
  if (value instanceof Error) return { code: 'invalid_input', message: value.message }
  return { code: 'unexpected', message: 'The operation could not be completed.' }
}

export interface HexSession {
  file: Ref<FileInfo | null>; page: Ref<ViewportPage | null>; selection: Ref<ByteSelection | null>
  sourceIdentity: Ref<number>
  template: Ref<TemplateDefinition>; results: Ref<ParsedField[]>; matches: Ref<bigint[]>
  searchMatchLength: Ref<number>; searchTruncated: Ref<boolean>
  activity: Ref<OperationActivity | null>
  busy: Record<BusyOperation, boolean>; progress: Ref<OperationProgress | null>; error: Ref<AppError | null>
  viewportOffset: Ref<bigint>; editMode: Ref<boolean>; canUndo: ComputedRef<boolean>
  requestPage(offset: bigint, length: number, generation?: number): Promise<void>
  openFile(path: string, discardUnsaved?: boolean): Promise<void>; closeFile(discardUnsaved?: boolean): Promise<void>; goTo(text: string): bigint
  search(text: string): Promise<void>; applyTemplate(): Promise<void>; editSelectedByte(text: string): Promise<void>
  undo(): Promise<void>; saveAs(path: string): Promise<void>; loadTemplate(path: string): Promise<void>
  saveTemplate(path: string): Promise<void>; exportCsv(path: string): Promise<void>
  prepareClose(): Promise<DirtyState>; releaseCloseBarrier(): void
  updateTemplate(template: TemplateDefinition): void; navigate(range: { start: bigint; end: bigint }): void
  clearSelection(): void; clearError(): void; presentError(error: unknown): void
}

interface OperationTicket { name: BusyOperation; token: number; epoch: number; issue: number; progressIssue: number; relevant: () => boolean }

export function useHexSession(api: HexBackend = defaultBackend): HexSession {
  const file = ref<FileInfo | null>(null)
  const sourceIdentity = ref(0)
  const page = ref<ViewportPage | null>(null)
  const selection = ref<ByteSelection | null>(null)
  const template = ref<TemplateDefinition>({ ...EMPTY_TEMPLATE, fields: [] })
  const results = ref<ParsedField[]>([])
  const matches = ref<bigint[]>([])
  const searchMatchLength = ref(1)
  const searchTruncated = ref(false)
  const progress = ref<OperationProgress | null>(null)
  const activity = ref<OperationActivity | null>(null)
  const error = ref<AppError | null>(null)
  const viewportOffset = ref(0n)
  const editMode = ref(false)
  const undoDepth = ref(0)
  const canUndo = computed(() => undoDepth.value > 0)
  const busy = reactive<Record<BusyOperation, boolean>>({ open: false, close: false, page: false, search: false, parse: false, edit: false, undo: false, save: false, template: false, export: false })
  const pending = reactive<Record<BusyOperation, number>>({ open: 0, close: 0, page: 0, search: 0, parse: 0, edit: 0, undo: 0, save: 0, template: 0, export: 0 })
  const tokens: Record<BusyOperation, number> = { open: 0, close: 0, page: 0, search: 0, parse: 0, edit: 0, undo: 0, save: 0, template: 0, export: 0 }
  let nextGeneration = 0
  let sessionEpoch = 0
  let latestIssue = 0
  let latestRelevantIssue = 0
  let latestProgressIssue = 0
  let contentVersion = 0
  let templateVersion = 0
  let viewportIntentVersion = 0
  let openQueue: Promise<void> | null = null
  const activities = new Map<number, OperationActivity>()
  const pendingMutations = new Set<Promise<unknown>>()
  let closeBarrier = false

  async function runMutation<T>(operation: () => Promise<T>): Promise<T> {
    if (closeBarrier) {
      const blocked = { code: 'operation_failed', message: 'Window close is in progress.' } satisfies AppError
      presentError(blocked)
      throw blocked
    }
    const promise = operation()
    pendingMutations.add(promise)
    try { return await promise }
    finally { pendingMutations.delete(promise) }
  }

  async function prepareClose(): Promise<DirtyState> {
    closeBarrier = true
    await Promise.allSettled([...pendingMutations])
    if (!file.value) return { dirty: false, revision: '0' }
    return api.getDirtyState()
  }

  function releaseCloseBarrier(): void { closeBarrier = false }

  function syncActivity(): void {
    let latest: [number, OperationActivity] | undefined
    for (const entry of activities) if (!latest || entry[0] > latest[0]) latest = entry
    activity.value = latest ? { operation: latest[1].operation, progress: latest[1].progress } : null
  }

  function isEpochCurrent(ticket: OperationTicket): boolean {
    return ticket.epoch === sessionEpoch
  }

  function isCurrent(ticket: OperationTicket): boolean {
    return (ticket.name === 'open' || isEpochCurrent(ticket)) && ticket.token === tokens[ticket.name] && ticket.relevant()
  }

  async function run<T>(
    name: BusyOperation,
    operation: (ticket: OperationTicket) => Promise<T>,
    tracksProgress = false,
    relevant: () => boolean = () => true,
  ): Promise<{ value: T; current: boolean; epochCurrent: boolean }> {
    const ticket: OperationTicket = { name, token: ++tokens[name], epoch: sessionEpoch, issue: ++latestIssue, progressIssue: tracksProgress ? ++latestProgressIssue : 0, relevant }
    pending[name] += 1
    busy[name] = pending[name] > 0
    if (name !== 'page') { activities.set(ticket.issue, { operation: name, progress: null }); syncActivity() }
    if (name !== 'page') { latestRelevantIssue = ticket.issue; error.value = null }
    if (tracksProgress) progress.value = null
    try {
      const value = await operation(ticket)
      return { value, current: isCurrent(ticket), epochCurrent: isEpochCurrent(ticket) }
    } catch (cause) {
      const normalized = friendlyError(cause)
      if (isCurrent(ticket) && ticket.issue >= latestRelevantIssue) { latestRelevantIssue = ticket.issue; error.value = normalized }
      throw normalized
    } finally {
      pending[name] -= 1
      busy[name] = pending[name] > 0
      activities.delete(ticket.issue)
      syncActivity()
    }
  }

  function reportProgress(ticket: OperationTicket, value: OperationProgress): void {
    if (!isCurrent(ticket)) return
    const active = activities.get(ticket.issue)
    if (active) { active.progress = value; syncActivity() }
    if (ticket.progressIssue === latestProgressIssue) progress.value = value
  }

  function updateFileState(state: DirtyState): void {
    if (file.value) file.value = { ...file.value, dirty: state.dirty, revision: state.revision }
  }

  function invalidateDerivedContent(): void {
    contentVersion += 1
    matches.value = []
    searchMatchLength.value = 1
    searchTruncated.value = false
    results.value = []
    progress.value = null
  }

  function installOpenedFile(opened: FileInfo): void {
    file.value = opened
    undoDepth.value = 0
    selection.value = null
    invalidateDerivedContent()
    viewportOffset.value = 0n
    page.value = null
  }

  async function loadPage(offset: bigint, length: number, generation: number): Promise<void> {
    nextGeneration = Math.max(nextGeneration, generation)
    const result = await run('page', () => api.readPage(offset, length))
    if (result.current) page.value = { ...result.value, generation }
  }

  async function requestPage(offset: bigint, length: number, generation = ++nextGeneration): Promise<void> {
    viewportIntentVersion += 1
    await loadPage(offset, length, generation)
  }

  async function refreshPage(expectedViewportIntent: number): Promise<void> {
    if (expectedViewportIntent !== viewportIntentVersion) return
    if (!page.value) return
    const current = page.value
    if (current.bytes.length === 0) return
    await loadPage(BigInt(current.offset), current.bytes.length, current.generation)
  }

  async function openFileCore(path: string, discardUnsaved = false): Promise<void> {
    contentVersion += 1
    const predecessor = openQueue
    const queuedOpen = predecessor
      ? (async () => { await predecessor; return api.openFile(path, discardUnsaved) })()
      : api.openFile(path, discardUnsaved)
    const queueEnd = queuedOpen.then(() => undefined, () => undefined)
    openQueue = queueEnd
    void queueEnd.then(() => { if (openQueue === queueEnd) openQueue = null })
    const result = await run('open', () => queuedOpen)
    sessionEpoch += 1
    sourceIdentity.value += 1
    installOpenedFile(result.value)
  }
  function openFile(path: string, discardUnsaved = false): Promise<void> { return runMutation(() => openFileCore(path, discardUnsaved)) }

  async function closeFileCore(discardUnsaved = false): Promise<void> {
    const result = await run('close', () => api.closeFile(discardUnsaved))
    if (!result.current) return
    sessionEpoch += 1
    sourceIdentity.value += 1
    file.value = null
    page.value = null
    selection.value = null
    undoDepth.value = 0
    editMode.value = false
    viewportOffset.value = 0n
    invalidateDerivedContent()
  }
  function closeFile(discardUnsaved = false): Promise<void> { return runMutation(() => closeFileCore(discardUnsaved)) }

  function navigate(range: { start: bigint; end: bigint }): void {
    selection.value = normalizeSelection(range.start, range.end)
    viewportOffset.value = range.start
  }

  function goTo(text: string): bigint {
    try {
      if (!file.value) throw new Error('Open a file before navigating.')
      const offset = parseOffset(text, BigInt(file.value.size))
      navigate({ start: offset, end: offset })
      latestRelevantIssue = ++latestIssue
      error.value = null
      return offset
    } catch (cause) { const normalized = friendlyError(cause); presentError(normalized); throw normalized }
  }

  async function search(text: string): Promise<void> {
    const searchedContent = contentVersion
    const pattern = parseHexBytes(text)
    const result = await run('search', (ticket) => {
      return api.searchBytes(text, (value) => reportProgress(ticket, value))
    }, true, () => searchedContent === contentVersion)
    if (!result.current) return
    matches.value = result.value.matches.map(BigInt)
    searchMatchLength.value = pattern.length
    searchTruncated.value = result.value.truncated
    const first = matches.value[0]
    if (first !== undefined) navigate({ start: first, end: first })
  }

  async function applyTemplate(): Promise<void> {
    const parsedTemplate = templateVersion
    const parsedContent = contentVersion
    const relevant = () => parsedTemplate === templateVersion && parsedContent === contentVersion
    const definition = backendTemplate(template.value)
    const result = await run('parse', (ticket) => api.applyTemplate(definition, (value) => reportProgress(ticket, value)), true, relevant)
    if (result.current) results.value = result.value
  }

  async function editSelectedByteCore(text: string): Promise<void> {
    let value: number
    try { value = parseSingleByte(text) }
    catch (cause) { const normalized = friendlyError(cause); presentError(normalized); throw normalized }
    const selected = selection.value
    if (!selected || selected.count !== 1n) { const normalized = friendlyError(new Error('Select exactly one byte to edit.')); presentError(normalized); throw normalized }
    const previousRevision = file.value?.revision
    const viewportIntent = viewportIntentVersion
    const result = await run('edit', () => api.editByte(selected.start, value))
    if (!result.epochCurrent) return
    if (previousRevision !== undefined && result.value.revision !== previousRevision) undoDepth.value += 1
    invalidateDerivedContent()
    updateFileState(result.value)
    await refreshPage(viewportIntent)
  }
  function editSelectedByte(text: string): Promise<void> { return runMutation(() => editSelectedByteCore(text)) }

  async function undoCore(): Promise<void> {
    const viewportIntent = viewportIntentVersion
    const result = await run('undo', () => api.undoEdit())
    if (!result.epochCurrent) return
    if (result.value.undone) { undoDepth.value = Math.max(0, undoDepth.value - 1); invalidateDerivedContent() }
    updateFileState(result.value)
    await refreshPage(viewportIntent)
  }
  function undo(): Promise<void> { return runMutation(undoCore) }

  async function saveAsCore(path: string): Promise<void> {
    const viewportIntent = viewportIntentVersion
    const result = await run('save', (ticket) => api.saveAs(path, (value) => reportProgress(ticket, value)), true)
    if (!result.epochCurrent) return
    undoDepth.value = 0
    invalidateDerivedContent()
    updateFileState(result.value)
    await refreshPage(viewportIntent)
  }
  function saveAs(path: string): Promise<void> { return runMutation(() => saveAsCore(path)) }

  async function loadTemplate(path: string): Promise<void> {
    const loadedTemplate = ++templateVersion
    progress.value = null
    const result = await run('template', () => api.loadTemplate(path), false, () => loadedTemplate === templateVersion)
    if (result.current) { template.value = result.value; results.value = [] }
  }
  async function saveTemplate(path: string): Promise<void> { await run('template', () => api.saveTemplate(path, backendTemplate(template.value))) }
  async function exportCsv(path: string): Promise<void> { await run('export', (ticket) => api.exportResultsCsv(path, backendTemplate(template.value), (value) => reportProgress(ticket, value)), true) }

  function presentError(cause: unknown): void {
    latestRelevantIssue = ++latestIssue
    error.value = friendlyError(cause)
  }

  function updateTemplate(value: TemplateDefinition): void {
    templateVersion += 1
    template.value = value
    results.value = []
    progress.value = null
  }

  return {
    file, page, selection, sourceIdentity, template, results, matches, searchMatchLength, searchTruncated, activity, busy, progress, error, viewportOffset, editMode, canUndo,
    requestPage, openFile, closeFile, goTo, search, applyTemplate, editSelectedByte, undo, saveAs, loadTemplate,
    saveTemplate, exportCsv, updateTemplate, navigate, clearSelection: () => { selection.value = null }, clearError: () => { error.value = null }, presentError,
    prepareClose, releaseCloseBarrier,
  }
}

function backendTemplate(value: TemplateDefinition): TemplateDefinition {
  return { ...value, fields: value.fields.map((field) => {
    const offset = field.offset.trim()
    if (!/^(?:0[xX][0-9a-fA-F]+|[0-9]+)$/.test(offset)) return { ...field }
    return { ...field, offset: BigInt(offset).toString() }
  }) }
}
