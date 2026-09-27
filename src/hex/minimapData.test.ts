import { describe, expect, it, vi } from 'vitest'
import type { MinimapSamplesResponse, PageResponse, ViewportPage } from '../types'
import { createMinimapGeometry } from './minimapGeometry'
import { createMinimapDataManager, type MinimapDataInput } from './minimapData'

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail })
  return { promise, resolve, reject }
}

function input(overrides: Partial<MinimapDataInput> = {}): MinimapDataInput {
  return {
    enabled: true,
    sourceIdentity: 1,
    sourceKey: 'a.bin',
    revision: '1',
    fileSize: 16_000n,
    bytesPerRow: 16,
    geometry: createMinimapGeometry({ mode: 'fit', totalRows: 1000n, heightPx: 70, rowPx: 2, topRow: 0n }),
    acceptedPage: { offset: '0', bytes: Array(16).fill(0x41), modifiedOffsets: [], revision: '1', generation: 1 },
    ...overrides,
  }
}

function response(rows: bigint[], revision = '1'): MinimapSamplesResponse {
  return { revision, samples: rows.map((row) => ({ row: row.toString(), bytes: [Number(row % 256n)], modifiedOffsets: [] })) }
}

describe('minimap data manager', () => {
  it('streams long fit samples in sequential batches of at most 32 rows', async () => {
    let active = 0; let maxActive = 0
    const calls: bigint[][] = []
    const api = {
      readPage: vi.fn<(_offset: bigint, _length: number) => Promise<PageResponse>>(),
      readMinimapSamples: vi.fn(async (rows: bigint[]) => {
        active += 1; maxActive = Math.max(maxActive, active); calls.push(rows)
        await Promise.resolve()
        active -= 1
        return response(rows)
      }),
    }
    const changed: bigint[][] = []
    const manager = createMinimapDataManager(api, (rows) => changed.push(rows), () => {})
    manager.update(input())
    await vi.waitFor(() => expect(calls.length).toBeGreaterThan(1))
    await vi.waitFor(() => expect(manager.getRow(999n)?.bytes).toEqual([231]))
    expect(maxActive).toBe(1)
    expect(calls.every((batch) => batch.length <= 32)).toBe(true)
    expect(changed.length).toBeGreaterThan(1)
    expect(api.readPage).not.toHaveBeenCalled()
    manager.dispose()
  })

  it('ignores an old sample response after the file identity changes', async () => {
    const oldBatch = deferred<MinimapSamplesResponse>()
    const calls: bigint[][] = []
    const api = {
      readPage: vi.fn<(_offset: bigint, _length: number) => Promise<PageResponse>>(),
      readMinimapSamples: vi.fn((rows: bigint[]) => {
        calls.push(rows)
        return calls.length === 1 ? oldBatch.promise : Promise.resolve({
          revision: '2', samples: rows.map((row) => ({ row: row.toString(), bytes: [0xbb], modifiedOffsets: [] })),
        })
      }),
    }
    const changed: bigint[][] = []
    const manager = createMinimapDataManager(api, (rows) => changed.push(rows), () => {})
    manager.update(input())
    await vi.waitFor(() => expect(calls).toHaveLength(1))
    manager.update(input({ sourceIdentity: 2, sourceKey: 'b.bin', revision: '2', acceptedPage: {
      offset: '0', bytes: Array(16).fill(0x42), modifiedOffsets: [], revision: '2', generation: 2,
    } }))
    oldBatch.resolve({ revision: '1', samples: calls[0]!.map((row) => ({ row: row.toString(), bytes: [0xaa], modifiedOffsets: [] })) })
    await vi.waitFor(() => expect(calls.length).toBeGreaterThan(1))
    await vi.waitFor(() => expect(manager.getRow(calls[0]![0]!)?.bytes).toEqual([0xbb]))
    expect(manager.getRow(0n)?.bytes).toEqual(Array(16).fill(0x42))
    expect(changed.length).toBeGreaterThan(0)
    manager.dispose()
  })

  it('retains unaffected cached rows across a known edit delta', async () => {
    const api = {
      readPage: vi.fn<(_offset: bigint, _length: number) => Promise<PageResponse>>(),
      readMinimapSamples: vi.fn(async (rows: bigint[], _width: number, _key: string, revision: string) => response(rows, revision)),
    }
    const manager = createMinimapDataManager(api, () => {}, () => {})
    const short = createMinimapGeometry({ mode: 'fit', totalRows: 4n, heightPx: 4, rowPx: 1, topRow: 0n })
    manager.update(input({ fileSize: 64n, geometry: short }))
    await vi.waitFor(() => expect(manager.getRow(2n)).not.toBeNull())
    const unaffected = manager.getRow(1n)
    manager.applyEditDelta({ offset: 32n, revision: '2' })
    manager.update(input({ fileSize: 64n, geometry: short, revision: '2', acceptedPage: null }))
    expect(manager.getRow(1n)).toEqual(unaffected)
    expect(manager.getRow(2n)).toBeNull()
    manager.dispose()
  })

  it('reports a current sample failure once without touching the main page', async () => {
    const page: ViewportPage = { offset: '0', bytes: Array(16).fill(0x41), modifiedOffsets: [], revision: '1', generation: 1 }
    const api = {
      readPage: vi.fn<(_offset: bigint, _length: number) => Promise<PageResponse>>(),
      readMinimapSamples: vi.fn().mockRejectedValue({ code: 'source_changed', message: 'Source changed.' }),
    }
    const errors: unknown[] = []
    const manager = createMinimapDataManager(api, () => {}, (error) => errors.push(error))
    const settings = input({ acceptedPage: page })
    manager.update(settings)
    await vi.waitFor(() => expect(errors).toHaveLength(1))
    manager.update(settings)
    expect(page.bytes).toEqual(Array(16).fill(0x41))
    expect(api.readMinimapSamples).toHaveBeenCalledTimes(1)
    manager.dispose()
  })

  it('waits for a page at the current revision before scheduling minimap work', async () => {
    const api = {
      readPage: vi.fn<(_offset: bigint, _length: number) => Promise<PageResponse>>(),
      readMinimapSamples: vi.fn(async (rows: bigint[]) => response(rows, '2')),
    }
    const manager = createMinimapDataManager(api, () => {}, () => {})
    manager.update(input({ revision: '2' }))
    await new Promise((resolve) => setTimeout(resolve, 5))
    expect(api.readMinimapSamples).not.toHaveBeenCalled()
    manager.dispose()
  })

  it('reuses an accepted page in proportional mode before requesting another page', async () => {
    const api = {
      readPage: vi.fn(async (offset: bigint): Promise<PageResponse> => ({
        offset: offset.toString(), bytes: Array(16 * 8).fill(0x43), modifiedOffsets: [], revision: '1',
      })),
      readMinimapSamples: vi.fn(),
    }
    const manager = createMinimapDataManager(api, () => {}, () => {})
    const geometry = createMinimapGeometry({ mode: 'proportional', totalRows: 1000n, heightPx: 8, rowPx: 1, topRow: 0n })
    manager.update(input({ geometry, acceptedPage: { offset: '0', bytes: Array(16 * 8).fill(0x42), modifiedOffsets: [], revision: '1', generation: 1 } }))
    expect(manager.getRow(7n)?.bytes).toEqual(Array(16).fill(0x42))
    expect(api.readPage).not.toHaveBeenCalled()
    manager.update(input({ geometry: createMinimapGeometry({ mode: 'proportional', totalRows: 1000n, heightPx: 8, rowPx: 1, topRow: 100n }) }))
    await vi.waitFor(() => expect(manager.getRow(100n)?.bytes).toEqual(Array(16).fill(0x43)))
    expect(api.readMinimapSamples).not.toHaveBeenCalled()
    manager.dispose()
  })

  it('retains overlapping proportional samples when the virtual window moves one row', () => {
    const api = { readPage: vi.fn(), readMinimapSamples: vi.fn() }
    const manager = createMinimapDataManager(api, () => {}, () => {})
    const first = createMinimapGeometry({ mode: 'proportional', totalRows: 1000n, heightPx: 8, rowPx: 1, topRow: 0n })
    const next = createMinimapGeometry({ mode: 'proportional', totalRows: 1000n, heightPx: 8, rowPx: 1, topRow: 1n })
    const acceptedPage = { offset: '0', bytes: Array(16 * 8).fill(0x42), modifiedOffsets: [], revision: '1', generation: 1 }
    manager.update(input({ geometry: first, acceptedPage }))
    const overlap = manager.getRow(1n)
    manager.update(input({ geometry: next, acceptedPage }))
    expect(manager.getRow(1n)).toBe(overlap)
    manager.dispose()
  })

  it('ignores a delayed proportional page from a superseded virtual window', async () => {
    const oldPage = deferred<PageResponse>()
    const newPage = deferred<PageResponse>()
    const api = {
      readPage: vi.fn().mockReturnValueOnce(oldPage.promise).mockReturnValueOnce(newPage.promise),
      readMinimapSamples: vi.fn(),
    }
    const manager = createMinimapDataManager(api, () => {}, () => {})
    const first = createMinimapGeometry({ mode: 'proportional', totalRows: 1000n, heightPx: 8, rowPx: 1, topRow: 0n })
    const next = createMinimapGeometry({ mode: 'proportional', totalRows: 1000n, heightPx: 8, rowPx: 1, topRow: 100n })
    const acceptedPage = { offset: '0', bytes: Array(16).fill(0x42), modifiedOffsets: [], revision: '1', generation: 1 }
    manager.update(input({ geometry: first, acceptedPage }))
    await vi.waitFor(() => expect(api.readPage).toHaveBeenCalledTimes(1))
    manager.update(input({ geometry: next, acceptedPage }))
    oldPage.resolve({ offset: '16', bytes: Array(16 * 8).fill(0xaa), modifiedOffsets: [], revision: '1' })
    await vi.waitFor(() => expect(api.readPage).toHaveBeenCalledTimes(2))
    expect(manager.getRow(1n)).toBeNull()
    newPage.resolve({ offset: '1600', bytes: Array(16 * 8).fill(0xbb), modifiedOffsets: [], revision: '1' })
    await vi.waitFor(() => expect(manager.getRow(100n)?.bytes[0]).toBe(0xbb))
    manager.dispose()
  })

  it('does not report a missing row when the main page wins the proportional read race', async () => {
    const slowPage = deferred<PageResponse>()
    const errors: unknown[] = []
    const api = { readPage: vi.fn(() => slowPage.promise), readMinimapSamples: vi.fn() }
    const manager = createMinimapDataManager(api, () => {}, (error) => errors.push(error))
    const geometry = createMinimapGeometry({ mode: 'proportional', totalRows: 1000n, heightPx: 8, rowPx: 1, topRow: 0n })
    const first = input({ geometry, acceptedPage: { offset: '0', bytes: Array(16).fill(0x41), modifiedOffsets: [], revision: '1', generation: 1 } })
    manager.update(first)
    await vi.waitFor(() => expect(api.readPage).toHaveBeenCalledTimes(1))
    manager.update(input({ geometry, acceptedPage: { offset: '0', bytes: Array(16 * 8).fill(0x42), modifiedOffsets: [], revision: '1', generation: 2 } }))
    slowPage.resolve({ offset: '16', bytes: Array(16 * 7).fill(0x42), modifiedOffsets: [], revision: '1' })
    await vi.waitFor(() => expect(manager.getRow(7n)?.bytes).toEqual(Array(16).fill(0x42)))
    await new Promise((resolve) => setTimeout(resolve, 5))
    expect(errors).toEqual([])
    manager.dispose()
  })

  it('ignores a stale proportional page when a fractional tile shift keeps the first row', async () => {
    const oldPage = deferred<PageResponse>()
    const newPage = deferred<PageResponse>()
    const api = { readPage: vi.fn().mockReturnValueOnce(oldPage.promise).mockReturnValueOnce(newPage.promise), readMinimapSamples: vi.fn() }
    const errors: unknown[] = []
    const manager = createMinimapDataManager(api, () => {}, (error) => errors.push(error))
    const make = (visualTopPx: number) => createMinimapGeometry({ mode: 'proportional', totalRows: 1000n,
      heightPx: 101, rowPx: 2, topRow: 495n, visibleMainRows: 10n, visualTopPx })
    const first = make(40.1)
    const second = make(40.11)
    expect(first.topRow).toBe(second.topRow)
    const acceptedPage = { offset: '0', bytes: Array(16).fill(0x41), modifiedOffsets: [], revision: '1', generation: 1 }
    manager.update(input({ geometry: first, acceptedPage }))
    await vi.waitFor(() => expect(api.readPage).toHaveBeenCalledTimes(1))
    manager.update(input({ geometry: second, acceptedPage }))
    oldPage.resolve({ offset: (first.topRow * 16n).toString(), bytes: Array(16 * 52).fill(0xaa), modifiedOffsets: [], revision: '1' })
    await vi.waitFor(() => expect(api.readPage).toHaveBeenCalledTimes(2))
    expect(manager.getRow(first.topRow)).toBeNull()
    newPage.resolve({ offset: (second.topRow * 16n).toString(), bytes: Array(16 * 52).fill(0xbb), modifiedOffsets: [], revision: '1' })
    await vi.waitFor(() => expect(manager.getRow(second.topRow)?.bytes[0]).toBe(0xbb))
    expect(errors).toEqual([])
    manager.dispose()
  })
})
