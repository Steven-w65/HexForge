import { describe, expect, it, vi } from 'vitest'
import type { PageResponse, TemplateDefinition } from '../types'
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

  it('keeps the newest overlapping open and ignores an older failure', async () => {
    const backend = fakeBackend(); const first = deferred<Awaited<ReturnType<HexBackend['openFile']>>>(); const second = deferred<Awaited<ReturnType<HexBackend['openFile']>>>()
    vi.mocked(backend.openFile).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
    const session = useHexSession(backend)
    const a = session.openFile('old.bin').catch(() => undefined); const b = session.openFile('new.bin')
    second.resolve({ name: 'new.bin', path: 'new.bin', size: '1', revision: '1', dirty: false }); await b
    expect(session.busy.open).toBe(true)
    first.reject({ code: 'old_failure', message: 'Old open failed.' }); await a
    expect(session.file.value?.name).toBe('new.bin'); expect(session.error.value).toBeNull(); expect(session.busy.open).toBe(false)
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
