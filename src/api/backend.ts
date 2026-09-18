import { Channel, invoke } from '@tauri-apps/api/core'
import type {
  AppError, DirtyState, FileInfo, OperationProgress, PageResponse, ParsedField,
  SaveResponse, SearchResponse, TemplateDefinition, UndoResponse,
} from '../types'

function normalizeError(error: unknown): AppError {
  if (typeof error === 'object' && error !== null &&
      'code' in error && typeof error.code === 'string' &&
      'message' in error && typeof error.message === 'string') {
    return error as AppError
  }
  return { code: 'unexpected', message: 'The operation could not be completed.' }
}

async function call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (error) {
    throw normalizeError(error)
  }
}

type ProgressHandler = (value: OperationProgress) => void

function withProgress<T>(command: string, args: Record<string, unknown>, onProgress: ProgressHandler): Promise<T> {
  const channel = new Channel<OperationProgress>()
  channel.onmessage = onProgress
  return call<T>(command, { ...args, onProgress: channel })
}

export const backend = {
  openFile: (path: string, discardUnsaved = false) => call<FileInfo>('open_file', { path, discardUnsaved }),
  closeFile: (discardUnsaved = false) => call<void>('close_file', { discardUnsaved }),
  getFileInfo: () => call<FileInfo>('get_file_info'),
  readPage: (offset: bigint, length: number) => call<PageResponse>('read_page', { offset: offset.toString(), length }),
  editByte: (offset: bigint, value: number) => call<DirtyState>('edit_byte', { offset: offset.toString(), value }),
  undoEdit: () => call<UndoResponse>('undo_edit'),
  getDirtyState: () => call<DirtyState>('get_dirty_state'),
  saveAs: (path: string, onProgress: ProgressHandler) => withProgress<SaveResponse>('save_as', { path }, onProgress),
  searchBytes: (pattern: string, onProgress: ProgressHandler) => withProgress<SearchResponse>('search_bytes', { pattern }, onProgress),
  applyTemplate: (template: TemplateDefinition, onProgress: ProgressHandler) => withProgress<ParsedField[]>('apply_template', { template }, onProgress),
  loadTemplate: (path: string) => call<TemplateDefinition>('load_template', { path }),
  saveTemplate: (path: string, template: TemplateDefinition) => call<void>('save_template', { path, template }),
  exportResultsCsv: (path: string, template: TemplateDefinition, onProgress: ProgressHandler) => withProgress<void>('export_results_csv', { path, template }, onProgress),
}
