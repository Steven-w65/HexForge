import { describe, expect, it, vi } from 'vitest'
import type { OperationProgress, PageResponse, ParsedField, TemplateDefinition } from '../types'
import { useHexSession, type HexBackend } from './useHexSession'

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail })
  return { promise, resolve, reject }
}

function pageAt(offset: bigint, revision = '1'): PageResponse {
  return { offset: offset.toString(), bytes: [0x41], modifiedOffsets: [], revision }
}

function fakeBackend(): HexBackend {
  return {
    openFile: vi.fn().mockResolvedValue({ name: 'input.bin', path: 'input.bin', size: '8192', revision: '1', dirty: false }),
    closeFile: vi.fn().mockResolvedValue(undefined), getFileInfo: vi.fn(),
    readPage: vi.fn().mockResolvedValue(pageAt(0n)), editByte: vi.fn().mockResolvedValue({ dirty: true, revision: '2' }),
    undoEdit: vi.fn().mockResolvedValue({ dirty: false, revision: '3', undone: true }), getDirtyState: vi.fn(),
    saveAs: vi.fn().mockResolvedValue({ dirty: false, revision: '4', bytesWritten: '8192', destination: 'copy.bin' }),
    searchBytes: vi.fn().mockResolvedValue({ matches: ['16', '32'], truncated: false }),
    applyTemplate: vi.fn().mockResolvedValue([]), loadTemplate: vi.fn(), saveTemplate: vi.fn(), exportResultsCsv: vi.fn(),
  }
}

describe('useHexSession', () => {
  it('ignores a page response older than the newest generation', async () => {
    const backend = fakeBackend()
    const first = deferred<PageResponse>(); const second = deferred<PageResponse>()
    vi.mocked(backend.readPage).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
    const session = useHexSession(backend)
    const a = session.requestPage(0n, 256, 1); const b = session.requestPage(4096n, 256, 2)
    second.resolve(pageAt(4096n)); await b
    first.resolve(pageAt(0n)); await a
    expect(session.page.value?.offset).toBe('4096')
    expect(session.page.value?.generation).toBe(2)
  })

  it('does not let a stale page failure replace the newest successful state', async () => {
    const backend = fakeBackend(); const first = deferred<PageResponse>(); const second = deferred<PageResponse>()
    vi.mocked(backend.readPage).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
    const session = useHexSession(backend)
    const a = session.requestPage(0n, 256, 1).catch(() => undefined)
    const b = session.requestPage(4096n, 256, 2)
    second.resolve(pageAt(4096n)); await b
    first.reject({ code: 'missing_file', message: 'Old request failed.' }); await a
    expect(session.page.value?.offset).toBe('4096'); expect(session.error.value).toBeNull()
    expect(session.busy.page).toBe(false)
  })

  it('does not let an edit refresh overwrite a newer viewport page', async () => {
    const backend = fakeBackend(); const refresh = deferred<PageResponse>(); const viewport = deferred<PageResponse>()
    vi.mocked(backend.readPage).mockReturnValueOnce(refresh.promise).mockReturnValueOnce(viewport.promise)
    const session = useHexSession(backend)
    session.file.value = { name: 'input.bin', path: 'input.bin', size: '8192', revision: '1', dirty: false }
    session.page.value = { ...pageAt(0n), generation: 1 }; session.selection.value = { start: 0n, end: 0n, count: 1n }
    const edit = session.editSelectedByte('FF')
    while (vi.mocked(backend.readPage).mock.calls.length < 1) await Promise.resolve()
    const move = session.requestPage(4096n, 256, 2)
    viewport.resolve(pageAt(4096n, '3')); await move
    refresh.resolve(pageAt(0n, '2')); await edit
    expect(session.page.value?.offset).toBe('4096')
  })

  it('does not start an edit refresh after a newer viewport intent already exists', async () => {
    const backend = fakeBackend(); const edited = deferred<{ dirty: boolean; revision: string }>(); const viewport = deferred<PageResponse>()
    vi.mocked(backend.editByte).mockReturnValue(edited.promise)
    vi.mocked(backend.readPage).mockReturnValueOnce(viewport.promise).mockResolvedValue(pageAt(0n, '2'))
    const session = useHexSession(backend)
    session.file.value = { name: 'input.bin', path: 'input.bin', size: '8192', revision: '1', dirty: false }
    session.page.value = { ...pageAt(0n), generation: 1 }; session.selection.value = { start: 0n, end: 0n, count: 1n }
    const edit = session.editSelectedByte('FF')
    const move = session.requestPage(4096n, 256, 2)
    edited.resolve({ dirty: true, revision: '2' }); await edit
    viewport.resolve(pageAt(4096n, '3')); await move
    expect(session.page.value?.offset).toBe('4096')
    expect(backend.readPage).toHaveBeenCalledTimes(1)
  })

  it('keeps overlapping search busy and ignores stale results, progress and failures', async () => {
    const backend = fakeBackend(); const first = deferred<{ matches: string[]; truncated: boolean }>(); const second = deferred<{ matches: string[]; truncated: boolean }>()
    let oldProgress!: (value: never) => void; let newProgress!: (value: never) => void
    vi.mocked(backend.searchBytes)
      .mockImplementationOnce((_pattern, progress) => { oldProgress = progress as never; return first.promise })
      .mockImplementationOnce((_pattern, progress) => { newProgress = progress as never; return second.promise })
    const session = useHexSession(backend)
    const a = session.search('AA').catch(() => undefined); const b = session.search('BB')
    newProgress({ operationId: 'new', phase: 'search', processed: '5', total: '10' } as never)
    oldProgress({ operationId: 'old', phase: 'search', processed: '9', total: '10' } as never)
    second.resolve({ matches: ['32'], truncated: false }); await b
    expect(session.busy.search).toBe(true); expect(session.progress.value?.operationId).toBe('new')
    first.reject({ code: 'old_failure', message: 'Old search failed.' }); await a
    expect(session.matches.value).toEqual([32n]); expect(session.error.value).toBeNull(); expect(session.busy.search).toBe(false)
  })

  it('keeps only the newest overlapping template load', async () => {
    const backend = fakeBackend(); const first = deferred<TemplateDefinition>(); const second = deferred<TemplateDefinition>()
    vi.mocked(backend.loadTemplate).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
    const session = useHexSession(backend)
    const a = session.loadTemplate('old.json'); const b = session.loadTemplate('new.json')
    second.resolve({ version: 1, name: 'New', defaultEndianness: 'little', fields: [] }); await b
    expect(session.busy.template).toBe(true)
    first.resolve({ version: 1, name: 'Old', defaultEndianness: 'big', fields: [] }); await a
    expect(session.template.value.name).toBe('New'); expect(session.busy.template).toBe(false)
  })

  it('does not publish parse results or progress after the template changes', async () => {
    const backend = fakeBackend(); const parsed = deferred<ParsedField[]>(); let parseProgress!: (value: OperationProgress) => void
    vi.mocked(backend.applyTemplate).mockImplementation((_template, progress) => { parseProgress = progress; return parsed.promise })
    vi.mocked(backend.loadTemplate).mockResolvedValue({ version: 1, name: 'B', defaultEndianness: 'big', fields: [] })
    const session = useHexSession(backend)
    session.template.value = { version: 1, name: 'A', defaultEndianness: 'little', fields: [] }
    const apply = session.applyTemplate()
    await session.loadTemplate('b.json')
    parseProgress({ operationId: 'old-parse', phase: 'parse', processed: '1', total: '1' })
    parsed.resolve([{ name: 'old', offset: '0', type: 'u8', length: 1, endianness: 'little', value: '41', comment: '' }]); await apply
    expect(session.template.value.name).toBe('B'); expect(session.results.value).toEqual([]); expect(session.progress.value).toBeNull()
  })

  it('does not publish search matches, progress or errors after bytes mutate', async () => {
    const backend = fakeBackend(); const found = deferred<{ matches: string[]; truncated: boolean }>(); let searchProgress!: (value: OperationProgress) => void
    vi.mocked(backend.searchBytes).mockImplementation((_pattern, progress) => { searchProgress = progress; return found.promise })
    const session = useHexSession(backend)
    session.file.value = { name: 'input.bin', path: 'input.bin', size: '8192', revision: '1', dirty: false }
    session.page.value = { ...pageAt(0n), generation: 1 }; session.selection.value = { start: 0n, end: 0n, count: 1n }
    const search = session.search('41')
    await session.editSelectedByte('42')
    searchProgress({ operationId: 'old-search', phase: 'search', processed: '1', total: '1' })
    found.resolve({ matches: ['0'], truncated: false }); await search
    expect(session.matches.value).toEqual([]); expect(session.progress.value).toBeNull(); expect(session.error.value).toBeNull()
  })

  it('suppresses a search failure racing the completion of a byte mutation', async () => {
    const backend = fakeBackend(); const found = deferred<{ matches: string[]; truncated: boolean }>(); const edited = deferred<{ dirty: boolean; revision: string }>()
    vi.mocked(backend.searchBytes).mockReturnValue(found.promise); vi.mocked(backend.editByte).mockReturnValue(edited.promise)
    const session = useHexSession(backend)
    session.file.value = { name: 'input.bin', path: 'input.bin', size: '8192', revision: '1', dirty: false }
    session.page.value = { ...pageAt(0n), generation: 1 }; session.selection.value = { start: 0n, end: 0n, count: 1n }
    const search = session.search('41').catch(() => undefined); const edit = session.editSelectedByte('42')
    edited.resolve({ dirty: true, revision: '2' }); found.reject({ code: 'old_search', message: 'Old search failed.' })
    await Promise.all([search, edit])
    expect(session.error.value).toBeNull()
  })

  it('serializes overlapping opens so backend and UI end on the latest requested file', async () => {
    const backend = fakeBackend(); const first = deferred<void>(); const second = deferred<void>()
    const started: string[] = []; let active = 0; let peakActive = 0; let backendFile = ''
    vi.mocked(backend.openFile).mockImplementation(async (path) => {
      started.push(path); active += 1; peakActive = Math.max(peakActive, active)
      await (path === 'old.bin' ? first.promise : second.promise)
      backendFile = path; active -= 1
      return { name: path, path, size: '1', revision: path === 'old.bin' ? '1' : '2', dirty: false }
    })
    const session = useHexSession(backend)
    const a = session.openFile('old.bin'); const b = session.openFile('new.bin')
    expect(started).toEqual(['old.bin'])
    first.resolve(); await a
    while (started.length < 2) await Promise.resolve()
    expect(started).toEqual(['old.bin', 'new.bin'])
    second.resolve(); await b
    expect(peakActive).toBe(1); expect(backendFile).toBe('new.bin'); expect(session.file.value?.name).toBe('new.bin')
    expect(session.error.value).toBeNull(); expect(session.busy.open).toBe(false)
  })

  it('reflects the last successful backend open when the newest queued open fails', async () => {
    const backend = fakeBackend(); const openedB = deferred<void>(); const openedC = deferred<void>(); let backendFile = 'a.bin'
    vi.mocked(backend.openFile).mockImplementation(async (path) => {
      if (path === 'b.bin') await openedB.promise
      else await openedC.promise
      if (path === 'c.bin') throw { code: 'permission_denied', message: 'C failed.' }
      backendFile = path
      return { name: path, path, size: '1', revision: '2', dirty: false }
    })
    vi.mocked(backend.readPage).mockImplementation(async () => ({ ...pageAt(0n), bytes: [backendFile === 'b.bin' ? 0x42 : 0x41] }))
    const session = useHexSession(backend)
    session.file.value = { name: 'a.bin', path: 'a.bin', size: '1', revision: '1', dirty: false }
    session.page.value = { ...pageAt(0n), generation: 1 }
    const b = session.openFile('b.bin'); const c = session.openFile('c.bin').catch(() => undefined)
    openedB.resolve(); await b
    expect(backendFile).toBe('b.bin'); expect(session.file.value?.name).toBe('b.bin'); expect(session.busy.open).toBe(true)
    openedC.resolve(); await c
    expect(backendFile).toBe('b.bin'); expect(session.file.value?.name).toBe('b.bin')
    expect(session.page.value?.bytes).toEqual([0x42]); expect(session.error.value?.message).toBe('C failed.')
  })

  it('invalidates a pending search when an older edit succeeds and a newer edit fails', async () => {
    const backend = fakeBackend(); const found = deferred<{ matches: string[]; truncated: boolean }>(); const firstEdit = deferred<{ dirty: boolean; revision: string }>(); const secondEdit = deferred<{ dirty: boolean; revision: string }>()
    vi.mocked(backend.searchBytes).mockReturnValue(found.promise)
    vi.mocked(backend.editByte).mockReturnValueOnce(firstEdit.promise).mockReturnValueOnce(secondEdit.promise)
    const session = useHexSession(backend)
    session.file.value = { name: 'input.bin', path: 'input.bin', size: '2', revision: '1', dirty: false }
    session.page.value = { ...pageAt(0n), generation: 1 }; session.selection.value = { start: 0n, end: 0n, count: 1n }
    const search = session.search('41'); const older = session.editSelectedByte('42'); const newer = session.editSelectedByte('43').catch(() => undefined)
    firstEdit.resolve({ dirty: true, revision: '2' }); await older
    secondEdit.reject({ code: 'edit_failed', message: 'Newer edit failed.' }); await newer
    found.resolve({ matches: ['0'], truncated: false }); await search
    expect(session.matches.value).toEqual([]); expect(session.error.value?.message).toBe('Newer edit failed.')
  })

  it('invalidates pending parse results when an older edit succeeds and newer undo is a no-op', async () => {
    const backend = fakeBackend(); const parsed = deferred<ParsedField[]>(); const firstEdit = deferred<{ dirty: boolean; revision: string }>(); const undo = deferred<{ dirty: boolean; revision: string; undone: boolean }>()
    vi.mocked(backend.applyTemplate).mockReturnValue(parsed.promise); vi.mocked(backend.editByte).mockReturnValue(firstEdit.promise); vi.mocked(backend.undoEdit).mockReturnValue(undo.promise)
    const session = useHexSession(backend)
    session.file.value = { name: 'input.bin', path: 'input.bin', size: '2', revision: '1', dirty: false }
    session.page.value = { ...pageAt(0n), generation: 1 }; session.selection.value = { start: 0n, end: 0n, count: 1n }
    const parse = session.applyTemplate(); const older = session.editSelectedByte('42'); const newer = session.undo()
    firstEdit.resolve({ dirty: true, revision: '2' }); await older
    undo.resolve({ dirty: true, revision: '2', undone: false }); await newer
    parsed.resolve([{ name: 'old', offset: '0', type: 'u8', length: 1, endianness: 'little', value: '41', comment: '' }]); await parse
    expect(session.results.value).toEqual([])
  })

  it('validates edit text before invoking Rust and refreshes the current page', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    session.file.value = { name: 'input.bin', path: 'input.bin', size: '8192', revision: '1', dirty: false }
    session.selection.value = { start: 7n, end: 7n, count: 1n }
    session.page.value = { ...pageAt(0n), generation: 1 }
    await expect(session.editSelectedByte('GG')).rejects.toThrow('exactly two')
    expect(backend.editByte).not.toHaveBeenCalled()
    await session.editSelectedByte('ff')
    expect(backend.editByte).toHaveBeenCalledWith(7n, 0xff)
    expect(backend.readPage).toHaveBeenCalledWith(0n, 1)
    expect(session.file.value?.dirty).toBe(true)
  })

  it('opens with a first page, resets view state, and always releases busy flags', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    session.matches.value = [9n]; session.results.value = [{ name: 'x' } as never]
    await session.openFile('input.bin')
    expect(session.matches.value).toEqual([]); expect(session.results.value).toEqual([])
    expect(backend.readPage).toHaveBeenCalledWith(0n, 1024 * 1024)
    expect(session.busy.open).toBe(false)
    vi.mocked(backend.openFile).mockRejectedValueOnce({ code: 'permission_denied', message: 'Permission denied.' })
    await expect(session.openFile('secret.bin')).rejects.toEqual(expect.objectContaining({ message: 'Permission denied.' }))
    expect(session.busy.open).toBe(false)
    expect(session.error.value?.message).toBe('Permission denied.')
  })

  it('validates navigation and search locally and navigates the first match', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    await session.openFile('input.bin')
    expect(session.goTo('0x10')).toBe(16n)
    expect(session.selection.value?.start).toBe(16n)
    await expect(session.search('4')).rejects.toThrow('two hexadecimal')
    expect(backend.searchBytes).not.toHaveBeenCalled()
    await session.search('41 42')
    expect(session.matches.value).toEqual([16n, 32n])
    expect(session.selection.value?.start).toBe(16n)
  })

  it('keeps dirty state backend-owned across undo and save', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    await session.openFile('input.bin')
    session.file.value!.dirty = true
    await session.undo(); expect(session.file.value!.dirty).toBe(false)
    session.file.value!.dirty = true
    await session.saveAs('copy.bin'); expect(session.file.value!.dirty).toBe(false)
    expect(session.file.value!.path).toBe('input.bin')
  })

  it('loads, saves, applies and exports the active flat template', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    const template: TemplateDefinition = { version: 1, name: 'Header', defaultEndianness: 'big', fields: [] }
    vi.mocked(backend.loadTemplate).mockResolvedValue(template)
    vi.mocked(backend.applyTemplate).mockResolvedValue([{ name: 'magic', offset: '0', type: 'u8', length: 1, endianness: 'big', value: '42', comment: '' }])
    await session.loadTemplate('header.json'); await session.saveTemplate('copy.json'); await session.applyTemplate(); await session.exportCsv('result.csv')
    expect(session.template.value).toEqual(template); expect(session.results.value).toHaveLength(1)
    expect(backend.exportResultsCsv).toHaveBeenCalledWith('result.csv', template, expect.any(Function))
  })
})
