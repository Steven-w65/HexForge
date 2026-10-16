/** Pending coverage is valid inventory, never evidence of a completed port. */
export function validateLedger(ledger, { requireComplete = false } = {}) {
  const errors = []
  if (!Array.isArray(ledger.entries) || ledger.entries.length === 0) return ['Ledger is empty']
  if (ledger.declaredCases !== ledger.entries.length) errors.push('Declared case count differs from inventory')
  const ids = new Set()
  for (const entry of ledger.entries) {
    if (ids.has(entry.id)) errors.push(`Duplicate assertion id: ${entry.id}`)
    ids.add(entry.id)
    if (!entry.id || !entry.source?.path || !entry.source?.name) errors.push(`Missing source identity: ${entry.id}`)
    if (requireComplete && (entry.status !== 'passed' || !entry.native?.test || !entry.native?.file)) errors.push(`Unmapped or unverified: ${entry.id}`)
    if (entry.status === 'passed' && (!entry.native?.evidence?.command || entry.native.evidence.result !== 'passed' || !entry.native.evidence.artifact)) errors.push(`Missing execution evidence: ${entry.id}`)
  }
  return errors
}

export function validateCapture(capture) {
  if (!capture.verified) return []
  const errors = []
  if (!Number.isFinite(capture.dpi) || capture.dpi <= 0) errors.push('Verified capture needs recorded DPI')
  if (!capture.state || !capture.theme || !capture.file) errors.push('Verified capture needs file, theme and state')
  if (!Array.isArray(capture.clientSize) || capture.clientSize.length !== 2 || capture.clientSize.some(value => !Number.isFinite(value) || value <= 0)) errors.push('Verified capture needs client size')
  if (capture.evidence !== 'lossless') errors.push('Verified pixel comparison needs lossless evidence')
  return errors
}
