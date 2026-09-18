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
  navigate(range: { start: bigint; end: bigint }): void; clearSelection(): void; clearError(): void
}

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
  let newestGeneration = 0
  let pageRequests = 0

  async function run<T>(name: BusyOperation, operation: () => Promise<T>): Promise<T> {
    busy[name] = true
    error.value = null
    try { return await operation() }
    catch (cause) { const normalized = friendlyError(cause); error.value = normalized; throw normalized }
    finally { busy[name] = false }
  }

  function updateFileState(state: DirtyState): void {
    if (file.value) file.value = { ...file.value, dirty: state.dirty, revision: state.revision }
  }

  async function requestPage(offset: bigint, length: number, generation = newestGeneration + 1): Promise<void> {
    newestGeneration = Math.max(newestGeneration, generation)
    pageRequests += 1
    busy.page = true
    if (generation === newestGeneration) error.value = null
    try {
      const response = await api.readPage(offset, length)
      if (generation !== newestGeneration) return
      page.value = { ...response, generation }
    } catch (cause) {
      if (generation !== newestGeneration) return
      const normalized = friendlyError(cause)
      error.value = normalized
      throw normalized
    } finally {
      pageRequests -= 1
      busy.page = pageRequests > 0
    }
  }

  async function refreshPage(): Promise<void> {
    if (!page.value) return
    const current = page.value
    const response = await api.readPage(BigInt(current.offset), current.bytes.length || FIRST_PAGE_LENGTH)
    page.value = { ...response, generation: current.generation }
  }

  async function openFile(path: string, discardUnsaved = false): Promise<void> {
    await run('open', async () => {
      const opened = await api.openFile(path, discardUnsaved)
      file.value = opened
      selection.value = null; matches.value = []; results.value = []; viewportOffset.value = 0n; page.value = null
      await requestPage(0n, FIRST_PAGE_LENGTH)
    })
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
      error.value = null
      return offset
    } catch (cause) { const normalized = friendlyError(cause); error.value = normalized; throw normalized }
  }

  async function search(text: string): Promise<void> {
    try { parseHexBytes(text) }
    catch (cause) { const normalized = friendlyError(cause); error.value = normalized; throw normalized }
    await run('search', async () => {
      const response = await api.searchBytes(text, (value) => { progress.value = value })
      matches.value = response.matches.map(BigInt)
      const first = matches.value[0]
      if (first !== undefined) navigate({ start: first, end: first })
    })
  }

  async function applyTemplate(): Promise<void> {
    await run('parse', async () => { results.value = await api.applyTemplate(template.value, (value) => { progress.value = value }) })
  }

  async function editSelectedByte(text: string): Promise<void> {
    let value: number
    try { value = parseSingleByte(text) }
    catch (cause) { const normalized = friendlyError(cause); error.value = normalized; throw normalized }
    const selected = selection.value
    if (!selected || selected.count !== 1n) { const normalized = friendlyError(new Error('Select exactly one byte to edit.')); error.value = normalized; throw normalized }
    await run('edit', async () => { updateFileState(await api.editByte(selected.start, value)); await refreshPage() })
  }

  async function undo(): Promise<void> {
    await run('undo', async () => { updateFileState(await api.undoEdit()); await refreshPage() })
  }

  async function saveAs(path: string): Promise<void> {
    await run('save', async () => { updateFileState(await api.saveAs(path, (value) => { progress.value = value })) })
  }

  async function loadTemplate(path: string): Promise<void> {
    await run('template', async () => { template.value = await api.loadTemplate(path); results.value = [] })
  }
  async function saveTemplate(path: string): Promise<void> { await run('template', () => api.saveTemplate(path, template.value)) }
  async function exportCsv(path: string): Promise<void> { await run('export', () => api.exportResultsCsv(path, template.value, (value) => { progress.value = value })) }

  return {
    file, page, selection, template, results, matches, busy, progress, error, viewportOffset, editMode,
    requestPage, openFile, goTo, search, applyTemplate, editSelectedByte, undo, saveAs, loadTemplate,
    saveTemplate, exportCsv, navigate, clearSelection: () => { selection.value = null }, clearError: () => { error.value = null },
  }
}
