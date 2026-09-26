import { describe, expect, it, vi } from 'vitest'
import type { OperationProgress, PageResponse, ParsedField, SaveResponse, TemplateDefinition } from '../types'
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
    saveAs: vi.fn().mockResolvedValue({ dirty: false, revision: '0', bytesWritten: '8192', destination: 'copy.bin',
      file: { name: 'copy.bin', path: 'copy.bin', size: '8192', revision: '0', dirty: false } }),
    searchBytes: vi.fn().mockResolvedValue({ matches: ['16', '32'], truncated: false }),
    applyTemplate: vi.fn().mockResolvedValue([]), loadTemplate: vi.fn(), saveTemplate: vi.fn(), exportResultsCsv: vi.fn(),
  }
}

describe('useHexSession', () => {
  it('advances source identity on every successful open even when metadata is unchanged', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    expect(session.sourceIdentity.value).toBe(0)
    await session.openFile('input.bin')
    expect(session.sourceIdentity.value).toBe(1)
    await session.openFile('input.bin')
    expect(session.sourceIdentity.value).toBe(2)
    expect(backend.readPage).not.toHaveBeenCalled()
  })

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

  it('unloads the template and ignores a parse completed afterward', async () => {
    const backend = fakeBackend(); const parsed = deferred<ParsedField[]>()
    vi.mocked(backend.applyTemplate).mockReturnValue(parsed.promise)
    const session = useHexSession(backend)
    session.updateTemplate({ version: 1, name: 'Header', defaultEndianness: 'big', fields: [
      { name: 'magic', offset: '0', type: 'u8', comment: '' },
    ] })
    const applying = session.applyTemplate()
    session.unloadTemplate()
    parsed.resolve([{ name: 'magic', offset: '0', type: 'u8', length: 1, endianness: 'big', value: '1', comment: '' }])
    await applying
    expect(session.template.value).toEqual({ version: 1, name: 'Untitled', defaultEndianness: 'little', fields: [] })
    expect(session.results.value).toEqual([])
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
    expect(session.page.value).toBeNull(); expect(backend.readPage).not.toHaveBeenCalled(); expect(session.error.value?.message).toBe('C failed.')
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

  it('ignores an edit result from file A after file B replaces the session', async () => {
    const backend = fakeBackend(); const editedA = deferred<{ dirty: boolean; revision: string }>()
    vi.mocked(backend.editByte).mockReturnValue(editedA.promise)
    vi.mocked(backend.openFile).mockResolvedValue({ name: 'b.bin', path: 'b.bin', size: '2', revision: 'b1', dirty: false })
    vi.mocked(backend.readPage).mockResolvedValue({ ...pageAt(0n, 'b1'), bytes: [0x42] })
    const session = useHexSession(backend)
    session.file.value = { name: 'a.bin', path: 'a.bin', size: '2', revision: 'a1', dirty: false }
    session.page.value = { ...pageAt(0n, 'a1'), generation: 1 }; session.selection.value = { start: 0n, end: 0n, count: 1n }
    const edit = session.editSelectedByte('42')
    await session.openFile('b.bin')
    session.matches.value = [1n]; session.results.value = [{ name: 'b-field' } as ParsedField]
    editedA.resolve({ dirty: true, revision: 'a2' }); await edit
    expect(session.file.value).toMatchObject({ name: 'b.bin', revision: 'b1', dirty: false })
    expect(session.matches.value).toEqual([1n]); expect(session.results.value).toEqual([{ name: 'b-field' }])
    expect(session.page.value).toBeNull(); expect(backend.readPage).not.toHaveBeenCalled()
  })

  it('ignores an undo result from file A after file B replaces the session', async () => {
    const backend = fakeBackend(); const undoneA = deferred<{ dirty: boolean; revision: string; undone: boolean }>()
    vi.mocked(backend.undoEdit).mockReturnValue(undoneA.promise)
    vi.mocked(backend.openFile).mockResolvedValue({ name: 'b.bin', path: 'b.bin', size: '2', revision: 'b1', dirty: true })
    const session = useHexSession(backend)
    session.file.value = { name: 'a.bin', path: 'a.bin', size: '2', revision: 'a1', dirty: true }
    session.page.value = { ...pageAt(0n, 'a1'), generation: 1 }
    const undo = session.undo()
    await session.openFile('b.bin')
    session.matches.value = [1n]
    undoneA.resolve({ dirty: false, revision: 'a2', undone: true }); await undo
    expect(session.file.value).toMatchObject({ name: 'b.bin', revision: 'b1', dirty: true })
    expect(session.matches.value).toEqual([1n]); expect(backend.readPage).not.toHaveBeenCalled()
  })

  it('ignores a Save As result from file A after file B replaces the session', async () => {
    const backend = fakeBackend(); const savedA = deferred<SaveResponse>()
    vi.mocked(backend.saveAs).mockReturnValue(savedA.promise)
    vi.mocked(backend.openFile).mockResolvedValue({ name: 'b.bin', path: 'b.bin', size: '2', revision: 'b1', dirty: true })
    const session = useHexSession(backend)
    session.file.value = { name: 'a.bin', path: 'a.bin', size: '2', revision: 'a1', dirty: true }
    const save = session.saveAs('a-copy.bin')
    await session.openFile('b.bin')
    savedA.resolve({ dirty: false, revision: '0', bytesWritten: '2', destination: 'a-copy.bin',
      file: { name: 'a-copy.bin', path: 'a-copy.bin', size: '2', revision: '0', dirty: false } }); await save
    expect(session.file.value).toMatchObject({ name: 'b.bin', revision: 'b1', dirty: true })
  })

  it('applies an edit result to file A when a replacement open fails', async () => {
    const backend = fakeBackend(); const editedA = deferred<{ dirty: boolean; revision: string }>(); const openedB = deferred<never>()
    vi.mocked(backend.editByte).mockReturnValue(editedA.promise); vi.mocked(backend.openFile).mockReturnValue(openedB.promise)
    const session = useHexSession(backend)
    session.file.value = { name: 'a.bin', path: 'a.bin', size: '2', revision: 'a1', dirty: false }
    session.page.value = { ...pageAt(0n, 'a1'), generation: 1 }; session.selection.value = { start: 0n, end: 0n, count: 1n }
    session.matches.value = [0n]; session.results.value = [{ name: 'a-field' } as ParsedField]
    const edit = session.editSelectedByte('42'); const open = session.openFile('b.bin').catch(() => undefined)
    openedB.reject({ code: 'permission_denied', message: 'B failed.' }); await open
    editedA.resolve({ dirty: true, revision: 'a2' }); await edit
    expect(session.file.value).toMatchObject({ name: 'a.bin', revision: 'a2', dirty: true })
    expect(session.matches.value).toEqual([]); expect(session.results.value).toEqual([])
  })

  it('applies an undo result to file A when a replacement open fails', async () => {
    const backend = fakeBackend(); const undoneA = deferred<{ dirty: boolean; revision: string; undone: boolean }>(); const openedB = deferred<never>()
    vi.mocked(backend.undoEdit).mockReturnValue(undoneA.promise); vi.mocked(backend.openFile).mockReturnValue(openedB.promise)
    const session = useHexSession(backend)
    session.file.value = { name: 'a.bin', path: 'a.bin', size: '2', revision: 'a1', dirty: true }
    session.page.value = { ...pageAt(0n, 'a1'), generation: 1 }; session.matches.value = [0n]
    const undo = session.undo(); const open = session.openFile('b.bin').catch(() => undefined)
    openedB.reject({ code: 'permission_denied', message: 'B failed.' }); await open
    undoneA.resolve({ dirty: false, revision: 'a2', undone: true }); await undo
    expect(session.file.value).toMatchObject({ name: 'a.bin', revision: 'a2', dirty: false })
    expect(session.matches.value).toEqual([])
  })

  it('applies a Save As result to file A when a replacement open fails', async () => {
    const backend = fakeBackend(); const savedA = deferred<SaveResponse>(); const openedB = deferred<never>()
    vi.mocked(backend.saveAs).mockReturnValue(savedA.promise); vi.mocked(backend.openFile).mockReturnValue(openedB.promise)
    const session = useHexSession(backend)
    session.file.value = { name: 'a.bin', path: 'a.bin', size: '2', revision: 'a1', dirty: true }
    const save = session.saveAs('a-copy.bin'); const open = session.openFile('b.bin').catch(() => undefined)
    openedB.reject({ code: 'permission_denied', message: 'B failed.' }); await open
    savedA.resolve({ dirty: false, revision: '0', bytesWritten: '2', destination: 'a-copy.bin',
      file: { name: 'a-copy.bin', path: 'a-copy.bin', size: '2', revision: '0', dirty: false } }); await save
    expect(session.file.value).toMatchObject({ name: 'a-copy.bin', revision: '0', dirty: false })
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

  it('opens metadata without preloading a page, resets view state, and always releases busy flags', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    session.matches.value = [9n]; session.results.value = [{ name: 'x' } as never]
    await session.openFile('input.bin')
    expect(session.matches.value).toEqual([]); expect(session.results.value).toEqual([])
    expect(session.page.value).toBeNull()
    expect(backend.readPage).not.toHaveBeenCalled()
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
    expect(session.file.value!.path).toBe('copy.bin')
  })

  it('invalidates the old page and derived state after Save As changes the active source', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    session.file.value = { name: 'input.bin', path: 'input.bin', size: '8192', revision: '3', dirty: true }
    session.page.value = { offset: '16', bytes: [0x99], modifiedOffsets: ['16'], revision: '3', generation: 7 }
    session.matches.value = [16n]; session.results.value = [{ name: 'stale' } as ParsedField]
    await session.saveAs('copy.bin')
    expect(session.file.value).toMatchObject({ path: 'copy.bin', dirty: false, revision: '0' })
    expect(session.page.value).toBeNull()
    expect(session.matches.value).toEqual([]); expect(session.results.value).toEqual([])
    expect(session.searchMatchLength.value).toBe(1); expect(session.searchTruncated.value).toBe(false)
    expect(backend.readPage).not.toHaveBeenCalled()
  })

  it('keeps the viewport and selection while switching to the saved copy', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    session.file.value = { name: 'source.bin', path: 'C:/source.bin', size: '8192', revision: '3', dirty: true }
    session.page.value = { offset: '16', bytes: [0x99], modifiedOffsets: ['16'], revision: '3', generation: 7 }
    session.selection.value = { start: 16n, end: 16n, count: 1n }
    session.viewportOffset.value = 16n
    vi.mocked(backend.saveAs).mockResolvedValue({
      dirty: false, revision: '0', bytesWritten: '8192', destination: 'C:/copy.bin',
      file: { name: 'copy.bin', path: 'C:/copy.bin', size: '8192', revision: '0', dirty: false },
    })

    await session.saveAs('C:/copy.bin')

    expect(session.file.value).toMatchObject({ name: 'copy.bin', path: 'C:/copy.bin', dirty: false })
    expect(session.sourceIdentity.value).toBe(1)
    expect(session.page.value).toBeNull()
    expect(session.viewportOffset.value).toBe(16n)
    expect(session.selection.value).toEqual({ start: 16n, end: 16n, count: 1n })
    expect(backend.readPage).not.toHaveBeenCalled()
  })

  it('keeps the visible activity label and progress owned by the same newest operation', async () => {
    const backend = fakeBackend(); const found = deferred<{ matches: string[]; truncated: boolean }>(); const saved = deferred<SaveResponse>()
    let searchProgress!: (value: OperationProgress) => void; let saveProgress!: (value: OperationProgress) => void
    vi.mocked(backend.searchBytes).mockImplementation((_pattern, progress) => { searchProgress = progress; return found.promise })
    vi.mocked(backend.saveAs).mockImplementation((_path, progress) => { saveProgress = progress; return saved.promise })
    const session = useHexSession(backend)
    const search = session.search('41'); searchProgress({ operationId: 'search', phase: 'search', processed: '4', total: '10' })
    const save = session.saveAs('copy.bin');
    expect(session.activity.value).toEqual({ operation: 'save', progress: null })
    saveProgress({ operationId: 'save', phase: 'save', processed: '2', total: '8' })
    searchProgress({ operationId: 'search', phase: 'search', processed: '9', total: '10' })
    expect(session.activity.value).toEqual({ operation: 'save', progress: expect.objectContaining({ operationId: 'save', processed: '2' }) })
    saved.resolve({ dirty: false, revision: '0', bytesWritten: '8', destination: 'copy.bin',
      file: { name: 'copy.bin', path: 'copy.bin', size: '8', revision: '0', dirty: false } }); await save
    expect(session.activity.value).toEqual({ operation: 'search', progress: expect.objectContaining({ operationId: 'search', processed: '9' }) })
    found.resolve({ matches: [], truncated: false }); await search
    expect(session.activity.value).toBeNull()
  })

  it('restores an older activity after a newer concurrent operation fails', async () => {
    const backend = fakeBackend(); const parsed = deferred<ParsedField[]>(); const exported = deferred<void>()
    let parseProgress!: (value: OperationProgress) => void
    vi.mocked(backend.applyTemplate).mockImplementation((_template, progress) => { parseProgress = progress; return parsed.promise })
    vi.mocked(backend.exportResultsCsv).mockReturnValue(exported.promise)
    const session = useHexSession(backend)
    const parse = session.applyTemplate(); parseProgress({ operationId: 'parse', phase: 'parse', processed: '1', total: '3' })
    const csv = session.exportCsv('out.csv').catch(() => undefined)
    expect(session.activity.value).toEqual({ operation: 'export', progress: null })
    exported.reject({ code: 'permission_denied', message: 'No output.' }); await csv
    expect(session.activity.value).toEqual({ operation: 'parse', progress: expect.objectContaining({ operationId: 'parse' }) })
    parsed.resolve([]); await parse
  })

  it('normalizes hexadecimal template offsets before backend calls', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    session.updateTemplate({ version: 1, name: 'Header', defaultEndianness: 'big', fields: [
      { name: 'magic', offset: '0x20', type: 'u16', endianness: 'big', comment: '' },
    ] })
    await session.applyTemplate(); await session.saveTemplate('header.json'); await session.exportCsv('header.csv')
    const expected = expect.objectContaining({ fields: [expect.objectContaining({ offset: '32' })] })
    expect(backend.applyTemplate).toHaveBeenCalledWith(expected, expect.any(Function))
    expect(backend.saveTemplate).toHaveBeenCalledWith('header.json', expected)
    expect(backend.exportResultsCsv).toHaveBeenCalledWith('header.csv', expected, expect.any(Function))
  })

  it('records the byte width and truncation status of the current search', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    vi.mocked(backend.searchBytes).mockResolvedValue({ matches: ['16'], truncated: true })
    await session.search('41 42 43')
    expect(session.searchMatchLength.value).toBe(3)
    expect(session.searchTruncated.value).toBe(true)
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

  it('records a successful empty parse as applied and invalidates it when the template changes', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    const template: TemplateDefinition = { version: 1, name: 'Draft', defaultEndianness: 'little', fields: [] }
    await session.openFile('input.bin')
    session.updateTemplate(template)
    expect(session.templateApplied.value).toBe(false)
    await session.applyTemplate()
    expect(session.results.value).toEqual([])
    expect(session.templateApplied.value).toBe(true)
    session.updateTemplate({ ...template, name: 'Revised' })
    expect(session.templateApplied.value).toBe(false)
  })

  it('invalidates applied status when file bytes or identity change, but not when parsing fails', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    await session.openFile('input.bin')
    await session.applyTemplate()
    session.selection.value = { start: 0n, end: 0n, count: 1n }
    await session.editSelectedByte('42')
    expect(session.templateApplied.value).toBe(false)
    await session.applyTemplate()
    vi.mocked(backend.applyTemplate).mockRejectedValueOnce({ code: 'invalid_template', message: 'Could not parse.' })
    await expect(session.applyTemplate()).rejects.toMatchObject({ code: 'invalid_template' })
    expect(session.templateApplied.value).toBe(true)
    await session.openFile('another.bin')
    expect(session.templateApplied.value).toBe(false)
    await session.applyTemplate()
    await session.saveAs('copy.bin')
    expect(session.templateApplied.value).toBe(false)
    await session.applyTemplate()
    await session.closeFile()
    expect(session.templateApplied.value).toBe(false)
  })

  it('marks parsed results as needing reapplication after a template change', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    const template: TemplateDefinition = { version: 1, name: 'Header', defaultEndianness: 'little', fields: [
      { name: 'magic', offset: '0', type: 'u8', comment: '' },
    ] }
    vi.mocked(backend.applyTemplate).mockResolvedValue([{ name: 'magic', offset: '0', type: 'u8', length: 1, endianness: 'little', value: '41', comment: '' }])
    session.updateTemplate(template)
    expect(session.resultsNeedRefresh.value).toBe(false)
    await session.applyTemplate()
    expect(session.resultsNeedRefresh.value).toBe(false)
    session.updateTemplate({ ...template, name: 'Header v2' })
    expect(session.results.value).toEqual([])
    expect(session.resultsNeedRefresh.value).toBe(true)
    await session.applyTemplate()
    expect(session.resultsNeedRefresh.value).toBe(false)
  })

  it('resets the reapply state when a different template is loaded', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    session.results.value = [{ name: 'old', offset: '0', type: 'u8', length: 1, endianness: 'little', value: '41', comment: '' }]
    session.updateTemplate({ version: 1, name: 'Changed', defaultEndianness: 'little', fields: [] })
    expect(session.resultsNeedRefresh.value).toBe(true)
    vi.mocked(backend.loadTemplate).mockResolvedValue({ version: 1, name: 'New', defaultEndianness: 'little', fields: [] })
    await session.loadTemplate('new.json')
    expect(session.resultsNeedRefresh.value).toBe(false)
  })

  it('barriers close behind an already-started edit before querying authoritative dirty state', async () => {
    const backend = fakeBackend(); const edited = deferred<{ dirty: boolean; revision: string }>()
    vi.mocked(backend.editByte).mockReturnValue(edited.promise)
    vi.mocked(backend.getDirtyState).mockResolvedValue({ dirty: true, revision: '2' })
    const session = useHexSession(backend)
    session.file.value = { name: 'input.bin', path: 'input.bin', size: '2', revision: '1', dirty: false }
    session.selection.value = { start: 0n, end: 0n, count: 1n }
    const edit = session.editSelectedByte('42')
    const preparing = session.prepareClose()
    await Promise.resolve()
    expect(backend.getDirtyState).not.toHaveBeenCalled()
    await expect(session.undo()).rejects.toMatchObject({ code: 'operation_failed' })
    expect(backend.undoEdit).not.toHaveBeenCalled()
    edited.resolve({ dirty: true, revision: '2' }); await edit
    expect(await preparing).toMatchObject({ dirty: true })
  })

  it('releases the close mutation barrier after cancellation or query failure', async () => {
    const backend = fakeBackend(); vi.mocked(backend.getDirtyState).mockRejectedValueOnce({ code: 'operation_failed', message: 'query failed' })
    const session = useHexSession(backend); session.file.value = { name: 'input.bin', path: 'input.bin', size: '2', revision: '1', dirty: false }
    await expect(session.prepareClose()).rejects.toMatchObject({ message: 'query failed' })
    session.releaseCloseBarrier()
    await session.undo()
    expect(backend.undoEdit).toHaveBeenCalledOnce()
  })

  it('keeps new mutations blocked after a clean query until close is released', async () => {
    const backend = fakeBackend(); vi.mocked(backend.getDirtyState).mockResolvedValue({ dirty: false, revision: '1' })
    const session = useHexSession(backend); session.file.value = { name: 'input.bin', path: 'input.bin', size: '2', revision: '1', dirty: false }
    expect(await session.prepareClose()).toMatchObject({ dirty: false })
    await expect(session.saveAs('copy.bin')).rejects.toMatchObject({ code: 'operation_failed' })
    expect(backend.saveAs).not.toHaveBeenCalled()
    session.releaseCloseBarrier(); await session.undo()
    expect(backend.undoEdit).toHaveBeenCalledOnce()
  })

  it('tracks whether the in-memory edit history can be undone and clears it after Save As', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    session.file.value = { name: 'input.bin', path: 'input.bin', size: '2', revision: '1', dirty: false }
    session.selection.value = { start: 0n, end: 0n, count: 1n }
    expect(session.canUndo.value).toBe(false)
    await session.editSelectedByte('42')
    expect(session.canUndo.value).toBe(true)
    await session.undo()
    expect(session.canUndo.value).toBe(false)
    vi.mocked(backend.editByte).mockResolvedValueOnce({ dirty: true, revision: '4' })
    await session.editSelectedByte('43')
    expect(session.canUndo.value).toBe(true)
    await session.saveAs('copy.bin')
    expect(session.canUndo.value).toBe(false)
  })

  it('closes the backend file and clears file-specific frontend state', async () => {
    const backend = fakeBackend(); const session = useHexSession(backend)
    await session.openFile('input.bin')
    session.selection.value = { start: 4n, end: 5n, count: 2n }
    session.matches.value = [4n]
    session.results.value = [{ name: 'x', offset: '4', type: 'u8', length: 1, endianness: 'little', value: '41', comment: '' }]
    session.viewportOffset.value = 4n
    session.editMode.value = true
    await session.closeFile(true)
    expect(backend.closeFile).toHaveBeenCalledWith(true)
    expect(session.file.value).toBeNull()
    expect(session.page.value).toBeNull()
    expect(session.selection.value).toBeNull()
    expect(session.matches.value).toEqual([])
    expect(session.results.value).toEqual([])
    expect(session.viewportOffset.value).toBe(0n)
    expect(session.editMode.value).toBe(false)
    expect(session.canUndo.value).toBe(false)
  })
})
