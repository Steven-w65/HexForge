import { reactive, ref, type Ref } from 'vue'
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

export type BusyOperation = 'open' | 'page' | 'search' | 'parse' | 'edit' | 'undo' | 'save' | 'template' | 'export'

const EMPTY_TEMPLATE: TemplateDefinition = { version: 1, name: 'Untitled', defaultEndianness: 'little', fields: [] }
const FIRST_PAGE_LENGTH = 1024 * 1024

function friendlyError(value: unknown): AppError {
  if (typeof value === 'object' && value !== null && 'code' in value && typeof value.code === 'string' &&
      'message' in value && typeof value.message === 'string') return value as AppError
  if (value instanceof Error) return { code: 'invalid_input', message: value.message }
  return { code: 'unexpected', message: 'The operation could not be completed.' }
}

export interface HexSession {
  file: Ref<FileInfo | null>; page: Ref<ViewportPage | null>; selection: Ref<ByteSelection | null>
  template: Ref<TemplateDefinition>; results: Ref<ParsedField[]>; matches: Ref<bigint[]>
  busy: Record<BusyOperation, boolean>; progress: Ref<OperationProgress | null>; error: Ref<AppError | null>
  viewportOffset: Ref<bigint>; editMode: Ref<boolean>
  requestPage(offset: bigint, length: number, generation?: number): Promise<void>
  openFile(path: string, discardUnsaved?: boolean): Promise<void>; goTo(text: string): bigint
  search(text: string): Promise<void>; applyTemplate(): Promise<void>; editSelectedByte(text: string): Promise<void>
  undo(): Promise<void>; saveAs(path: string): Promise<void>; loadTemplate(path: string): Promise<void>
  saveTemplate(path: string): Promise<void>; exportCsv(path: string): Promise<void>
  navigate(range: { start: bigint; end: bigint }): void; clearSelection(): void; clearError(): void; presentError(error: unknown): void
}

interface OperationTicket { name: BusyOperation; token: number; epoch: number; issue: number; progressIssue: number }

export function useHexSession(api: HexBackend = defaultBackend): HexSession {
  const file = ref<FileInfo | null>(null)
  const page = ref<ViewportPage | null>(null)
  const selection = ref<ByteSelection | null>(null)
  const template = ref<TemplateDefinition>({ ...EMPTY_TEMPLATE, fields: [] })
  const results = ref<ParsedField[]>([])
  const matches = ref<bigint[]>([])
  const progress = ref<OperationProgress | null>(null)
  const error = ref<AppError | null>(null)
  const viewportOffset = ref(0n)
  const editMode = ref(false)
  const busy = reactive<Record<BusyOperation, boolean>>({ open: false, page: false, search: false, parse: false, edit: false, undo: false, save: false, template: false, export: false })
  const pending = reactive<Record<BusyOperation, number>>({ open: 0, page: 0, search: 0, parse: 0, edit: 0, undo: 0, save: 0, template: 0, export: 0 })
  const tokens: Record<BusyOperation, number> = { open: 0, page: 0, search: 0, parse: 0, edit: 0, undo: 0, save: 0, template: 0, export: 0 }
  let nextGeneration = 0
  let sessionEpoch = 0
  let latestIssue = 0
  let latestRelevantIssue = 0
  let latestProgressIssue = 0
  let mutationVersion = 0

  function isCurrent(ticket: OperationTicket): boolean {
    return ticket.epoch === sessionEpoch && ticket.token === tokens[ticket.name]
  }

  async function run<T>(name: BusyOperation, operation: (ticket: OperationTicket) => Promise<T>, tracksProgress = false): Promise<{ value: T; current: boolean }> {
    const ticket: OperationTicket = { name, token: ++tokens[name], epoch: sessionEpoch, issue: ++latestIssue, progressIssue: tracksProgress ? ++latestProgressIssue : 0 }
    pending[name] += 1
    busy[name] = pending[name] > 0
    if (name !== 'page') { latestRelevantIssue = ticket.issue; error.value = null }
    if (tracksProgress) progress.value = null
    try {
      const value = await operation(ticket)
      return { value, current: isCurrent(ticket) }
    } catch (cause) {
      const normalized = friendlyError(cause)
      if (isCurrent(ticket) && ticket.issue >= latestRelevantIssue) { latestRelevantIssue = ticket.issue; error.value = normalized }
      throw normalized
    } finally {
      pending[name] -= 1
      busy[name] = pending[name] > 0
    }
  }

  function reportProgress(ticket: OperationTicket, value: OperationProgress): void {
    if (isCurrent(ticket) && ticket.progressIssue === latestProgressIssue) progress.value = value
  }

  function updateFileState(state: DirtyState): void {
    if (file.value) file.value = { ...file.value, dirty: state.dirty, revision: state.revision }
  }

  async function requestPage(offset: bigint, length: number, generation = ++nextGeneration): Promise<void> {
    nextGeneration = Math.max(nextGeneration, generation)
    const result = await run('page', () => api.readPage(offset, length))
    if (result.current) page.value = { ...result.value, generation }
  }

  async function refreshPage(): Promise<void> {
    if (!page.value) return
    const current = page.value
    await requestPage(BigInt(current.offset), current.bytes.length || FIRST_PAGE_LENGTH, current.generation)
  }

  async function openFile(path: string, discardUnsaved = false): Promise<void> {
    sessionEpoch += 1
    mutationVersion += 1
    const result = await run('open', () => api.openFile(path, discardUnsaved))
    if (!result.current) return
    file.value = result.value
    selection.value = null; matches.value = []; results.value = []; viewportOffset.value = 0n; page.value = null
    await requestPage(0n, FIRST_PAGE_LENGTH)
  }

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
    const result = await run('search', (ticket) => {
      parseHexBytes(text)
      return api.searchBytes(text, (value) => reportProgress(ticket, value))
    }, true)
    if (!result.current) return
    matches.value = result.value.matches.map(BigInt)
    const first = matches.value[0]
    if (first !== undefined) navigate({ start: first, end: first })
  }

  async function applyTemplate(): Promise<void> {
    const result = await run('parse', (ticket) => api.applyTemplate(template.value, (value) => reportProgress(ticket, value)), true)
    if (result.current) results.value = result.value
  }

  async function editSelectedByte(text: string): Promise<void> {
    let value: number
    try { value = parseSingleByte(text) }
    catch (cause) { const normalized = friendlyError(cause); presentError(normalized); throw normalized }
    const selected = selection.value
    if (!selected || selected.count !== 1n) { const normalized = friendlyError(new Error('Select exactly one byte to edit.')); presentError(normalized); throw normalized }
    const mutation = ++mutationVersion
    const result = await run('edit', () => api.editByte(selected.start, value))
    if (!result.current || mutation !== mutationVersion) return
    updateFileState(result.value)
    await refreshPage()
  }

  async function undo(): Promise<void> {
    const mutation = ++mutationVersion
    const result = await run('undo', () => api.undoEdit())
    if (!result.current || mutation !== mutationVersion) return
    updateFileState(result.value)
    await refreshPage()
  }

  async function saveAs(path: string): Promise<void> {
    const mutation = ++mutationVersion
    const result = await run('save', (ticket) => api.saveAs(path, (value) => reportProgress(ticket, value)), true)
    if (result.current && mutation === mutationVersion) updateFileState(result.value)
  }

  async function loadTemplate(path: string): Promise<void> {
    const result = await run('template', () => api.loadTemplate(path))
    if (result.current) { template.value = result.value; results.value = [] }
  }
  async function saveTemplate(path: string): Promise<void> { await run('template', () => api.saveTemplate(path, template.value)) }
  async function exportCsv(path: string): Promise<void> { await run('export', (ticket) => api.exportResultsCsv(path, template.value, (value) => reportProgress(ticket, value)), true) }

  function presentError(cause: unknown): void {
    latestRelevantIssue = ++latestIssue
    error.value = friendlyError(cause)
  }

  return {
    file, page, selection, template, results, matches, busy, progress, error, viewportOffset, editMode,
    requestPage, openFile, goTo, search, applyTemplate, editSelectedByte, undo, saveAs, loadTemplate,
    saveTemplate, exportCsv, navigate, clearSelection: () => { selection.value = null }, clearError: () => { error.value = null }, presentError,
  }
}
