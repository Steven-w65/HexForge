import type { TemplateSource } from '../types'

/** Persistence and parse freshness are independent: Save never implies Apply. */
export function templateStatus(state: { source: TemplateSource; dirty: boolean; applied: boolean; needsRefresh: boolean; hasFile: boolean; hasDiagnostics: boolean }): string {
  if (state.source === 'none') return 'No template'
  const persistence = state.source === 'draft' ? 'Unsaved draft' : state.dirty ? 'Modified' : 'Saved'
  const parse = !state.hasFile ? state.source === 'file' ? 'Loaded' : '' : state.applied
    ? state.hasDiagnostics ? 'Parse errors' : 'Applied' : state.needsRefresh ? 'Needs apply' : 'Pending apply'
  return [persistence, parse].filter(Boolean).join(' · ')
}
