import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { OperationProgress, TemplateDefinition } from '../types'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  Channel: class { onmessage?: (value: OperationProgress) => void },
}))
import { backend } from './backend'

const template: TemplateDefinition = {
  version: 1, name: 'Header', defaultEndianness: 'little', fields: [],
}

describe('backend IPC contract', () => {
  beforeEach(() => { invoke.mockReset() })

  it('serializes bigint offsets without losing precision', async () => {
    const page = { offset: '9007199254740993', bytes: [1], modifiedOffsets: [], revision: '1' }
    invoke.mockResolvedValue(page)
    expect(await backend.readPage(9007199254740993n, 1)).toEqual(page)
    expect(invoke).toHaveBeenCalledWith('read_page', { offset: '9007199254740993', length: 1 })
    expect(() => JSON.stringify(invoke.mock.calls)).not.toThrow()
  })

  it('routes file lifecycle, edits and template IO with camelCase arguments', async () => {
    invoke.mockResolvedValue(undefined)
    await backend.openFile('input.bin')
    await backend.openFile('replacement.bin', true)
    await backend.closeFile()
    await backend.closeFile(true)
    await backend.getFileInfo()
    await backend.editByte(18446744073709551615n, 254)
    await backend.undoEdit()
    await backend.getDirtyState()
    await backend.loadTemplate('header.json')
    await backend.saveTemplate('copy.json', template)
    expect(invoke.mock.calls).toEqual([
      ['open_file', { path: 'input.bin', discardUnsaved: false }],
      ['open_file', { path: 'replacement.bin', discardUnsaved: true }],
      ['close_file', { discardUnsaved: false }],
      ['close_file', { discardUnsaved: true }],
      ['get_file_info', {}],
      ['edit_byte', { offset: '18446744073709551615', value: 254 }],
      ['undo_edit', {}],
      ['get_dirty_state', {}],
      ['load_template', { path: 'header.json' }],
      ['save_template', { path: 'copy.json', template }],
    ])
    expect(() => JSON.stringify(invoke.mock.calls)).not.toThrow()
  })

  it('routes every long operation and forwards channel progress', async () => {
    const received: OperationProgress[] = []
    const progress: OperationProgress = { operationId: '1', phase: 'search', processed: '5', total: '10' }
    invoke.mockImplementation(async (_command, args) => {
      args.onProgress.onmessage(progress)
      return { ok: true }
    })
    const onProgress = (value: OperationProgress) => received.push(value)
    await backend.saveAs('copy.bin', onProgress)
    await backend.searchBytes('AA BB', onProgress)
    await backend.applyTemplate(template, onProgress)
    await backend.exportResultsCsv('results.csv', template, onProgress)
    expect(invoke.mock.calls.map(([command, { onProgress: channel, ...args }]) => [command, args, typeof channel.onmessage])).toEqual([
      ['save_as', { path: 'copy.bin' }, 'function'],
      ['search_bytes', { pattern: 'AA BB' }, 'function'],
      ['apply_template', { template }, 'function'],
      ['export_results_csv', { path: 'results.csv', template }, 'function'],
    ])
    expect(received).toEqual([progress, progress, progress, progress])
  })

  it('preserves structured errors and sanitizes unknown rejection payloads', async () => {
    const error = { code: 'invalid_offset', message: 'Invalid offset.', detail: null }
    invoke.mockRejectedValueOnce(error)
    await expect(backend.readPage(0n, 1)).rejects.toEqual(error)
    for (const failure of ['secret path', new Error('secret bytes'), null, { code: 7, message: 'bad' }]) {
      invoke.mockRejectedValueOnce(failure)
      await expect(backend.getFileInfo()).rejects.toEqual({ code: 'unexpected', message: 'The operation could not be completed.' })
    }
    invoke.mockRejectedValueOnce('secret')
    await expect(backend.searchBytes('AA', () => {})).rejects.toEqual({ code: 'unexpected', message: 'The operation could not be completed.' })
  })
})
