import type { ColorTheme, ParsedResult, TemplateDefinition } from '../types'

export const EDITOR_LABEL = 'template-editor'
export const MAIN_LABEL = 'main'
export const events = {
  ready: 'hexforge:template:ready', snapshot: 'hexforge:template:snapshot', draft: 'hexforge:template:draft',
  ack: 'hexforge:template:ack', action: 'hexforge:template:action', reply: 'hexforge:template:reply',
  flush: 'hexforge:template:flush', flushReply: 'hexforge:template:flush-reply', closed: 'hexforge:template:closed',
} as const

export interface TemplateSnapshot {
  revision: number
  ackSequence: number
  template: TemplateDefinition
  results: ParsedResult[]
  fileSize: string | null // Decimal strings keep multi-GB offsets JSON-safe.
  theme: ColorTheme
  dirty: boolean
  canApply: boolean
  busy?: boolean // Authoritative workflow/operation lock, independent of draft validity.
  applied?: boolean
  resultsNeedRefresh?: boolean
  hasDiagnostics?: boolean
  notice?: { id: number; text: string } | null
  active: boolean
  templateFilePath: string | null
  persistenceRevision: number
  workspaceRevision: number // Changes only when the authoritative template workspace is replaced.
  checkpointTemplate: TemplateDefinition
}

export interface DraftUpdate { sessionId: string; sequence: number; workspaceRevision: number; template: TemplateDefinition; valid: boolean }
export type EditorActionName = 'load' | 'save' | 'save-as' | 'apply' | 'unload' | 'navigate' | 'discard' | 'close'
export interface EditorAction { requestId: number; command: EditorActionName; draft: DraftUpdate; range?: { start: string; end: string } }
export interface EditorReply { requestId: number; ok: boolean; completed?: boolean; error?: string; snapshot?: TemplateSnapshot }

export interface LocalBus {
  listen(event: string, callback: (payload: unknown) => void | Promise<void>): Promise<() => void>
  send(target: string, event: string, payload: unknown): Promise<void>
}

export function errorMessage(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'message' in error && typeof error.message === 'string') return error.message
  return typeof error === 'string' ? error : 'The template action could not be completed.'
}
