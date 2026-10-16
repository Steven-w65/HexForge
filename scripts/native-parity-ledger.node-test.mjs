import test from 'node:test'
import assert from 'node:assert/strict'
import { validateLedger, validateCapture } from './native-parity-ledger.mjs'

const pending = () => ({ id: 'src/hex/layout.test.ts#group gaps#1', source: { path: 'src/hex/layout.test.ts', name: 'group gaps' }, status: 'pending', native: null })
const ledger = entries => ({ baseline: 'f796101', declaredCases: entries.length, entries })

test('duplicate_assertion_ids_are_rejected', () => {
  const errors = validateLedger(ledger([pending(), pending()]), { requireComplete: false })
  assert.ok(errors.some(error => error.includes('Duplicate assertion id')))
})

test('missing_native_mapping_is_rejected', () => {
  assert.ok(validateLedger(ledger([pending()]), { requireComplete: true }).some(error => error.includes('Unmapped')))
  assert.deepEqual(validateLedger(ledger([pending()]), { requireComplete: false }), [])
})

test('passed_status_requires_executing_native_evidence', () => {
  const entry = { ...pending(), status: 'passed', native: { test: 'group_gaps_and_eof_are_not_bytes', file: 'tests/hex_geometry.rs' } }
  assert.ok(validateLedger(ledger([entry]), { requireComplete: true }).some(error => error.includes('execution evidence')))
  entry.native.evidence = { command: 'cargo test --test hex_geometry', result: 'passed', artifact: 'hex-geometry.log' }
  assert.deepEqual(validateLedger(ledger([entry]), { requireComplete: true }), [])
})

test('declared_case_count_prevents_silently_dropping_assertions', () => {
  assert.ok(validateLedger({ ...ledger([pending()]), declaredCases: 2 }).some(error => error.includes('Declared case count')))
  assert.ok(validateLedger(ledger([])).some(error => error.includes('empty')))
})

test('visual_capture_requires_dpi_and_state', () => {
  const capture = { file: 'main.png', theme: 'dark', state: 'empty', clientSize: [1280, 800], dpi: 96, evidence: 'lossless', verified: true }
  assert.deepEqual(validateCapture(capture), [])
  assert.ok(validateCapture({ ...capture, dpi: null }).some(error => error.includes('DPI')))
  assert.ok(validateCapture({ ...capture, state: '' }).some(error => error.includes('state')))
  assert.ok(validateCapture({ ...capture, evidence: 'jpeg' }).some(error => error.includes('lossless')))
})

test('reference_jpeg_without_recorded_dpi_remains_unverified', () => {
  assert.deepEqual(validateCapture({ file: 'reference.jpg', verified: false, evidence: 'jpeg', dpi: null, state: 'empty' }), [])
})
