import type { MinimapSamplesResponse, PageResponse, ViewportPage } from '../types'
import type { BytesPerRow } from './layout'
import { samplePlan, type MinimapGeometry } from './minimapGeometry'

export interface MinimapDataInput {
  enabled: boolean
  sourceIdentity: number
  sourceKey: string
  revision: string
  fileSize: bigint
  bytesPerRow: BytesPerRow
  geometry: MinimapGeometry
  acceptedPage: ViewportPage | null
}

export interface MinimapRow {
  row: bigint
  bytes: number[]
  modifiedOffsets: bigint[]
}

export interface MinimapReadApi {
  readPage(offset: bigint, length: number): Promise<PageResponse>
  readMinimapSamples(rows: bigint[], bytesPerRow: BytesPerRow, sourceKey: string, expectedRevision: string): Promise<MinimapSamplesResponse>
}

export interface MinimapDataManager {
  update(input: MinimapDataInput): void
  getRow(row: bigint): MinimapRow | null
  applyEditDelta(delta: { offset: bigint; revision: string }): void
  retry(): void
  dispose(): void
}

function geometryKey(g: MinimapGeometry): string {
  // A virtual-window scroll retains overlapping rows. Only raster-shape
  // changes invalidate the cache; update() prunes rows outside the new plan.
  return `${g.mode}|${g.totalRows}|${g.heightPx}|${g.rowPx}`
}

/** Main pages are accepted first; sparse file-wide work is scheduled later. */
export function createMinimapDataManager(
  api: MinimapReadApi,
  onRowsChanged: (rows: bigint[]) => void,
  onError: (error: unknown) => void,
): MinimapDataManager {
  const cache = new Map<bigint, MinimapRow>()
  let current: MinimapDataInput | null = null
  let key = ''
  let generation = 0
  let pendingDelta: { offset: bigint; revision: string } | null = null
  let running = false
  let scheduled: ReturnType<typeof setTimeout> | null = null
  let failedGeneration = -1
  let disposed = false

  function plannedRows(input: MinimapDataInput): bigint[] {
    return [...new Set(samplePlan(input.geometry).map((position) => position.row))]
  }

  function hasCurrentPage(input: MinimapDataInput | null): input is MinimapDataInput & { acceptedPage: ViewportPage } {
    return !!input?.acceptedPage && input.acceptedPage.revision === input.revision
  }

  function addPage(page: PageResponse, input: MinimapDataInput): void {
    if (page.revision !== input.revision) return
    const start = BigInt(page.offset)
    const end = start + BigInt(page.bytes.length)
    const modified = new Set(page.modifiedOffsets.map(BigInt))
    const changed: bigint[] = []
    for (const row of plannedRows(input)) {
      const offset = row * BigInt(input.bytesPerRow)
      if (offset < start || offset >= input.fileSize) continue
      const needed = Number((input.fileSize - offset) < BigInt(input.bytesPerRow) ? input.fileSize - offset : BigInt(input.bytesPerRow))
      if (offset + BigInt(needed) > end) continue
      const at = Number(offset - start)
      const bytes = page.bytes.slice(at, at + needed)
      const modifiedOffsets: bigint[] = []
      for (let index = 0; index < needed; index += 1) if (modified.has(offset + BigInt(index))) modifiedOffsets.push(offset + BigInt(index))
      const prior = cache.get(row)
      if (prior && prior.bytes.length === bytes.length && prior.bytes.every((value, index) => value === bytes[index]) &&
          prior.modifiedOffsets.length === modifiedOffsets.length && prior.modifiedOffsets.every((value, index) => value === modifiedOffsets[index])) continue
      cache.set(row, { row, bytes, modifiedOffsets })
      changed.push(row)
    }
    if (changed.length) onRowsChanged(changed)
  }

  function schedule(): void {
    if (disposed || running || scheduled || !current?.enabled || !hasCurrentPage(current) || failedGeneration === generation) return
    scheduled = setTimeout(() => { scheduled = null; void pump() }, 0)
  }

  async function pump(): Promise<void> {
    if (running || disposed || !current?.enabled || !hasCurrentPage(current)) return
    const input = current
    const ticket = generation
    const missing = plannedRows(input).filter((row) => !cache.has(row))
    if (missing.length === 0) return
    running = true
    try {
      if (input.geometry.mode === 'fit') {
        const rows = missing.slice(0, 32)
        const result = await api.readMinimapSamples(rows, input.bytesPerRow, input.sourceKey, input.revision)
        if (disposed || ticket !== generation) return
        if (result.revision !== input.revision) throw new Error('The minimap response is no longer current.')
        const requested = new Set(rows)
        const changed: bigint[] = []
        for (const sample of result.samples) {
          const row = BigInt(sample.row)
          if (!requested.has(row)) continue
          cache.set(row, { row, bytes: sample.bytes, modifiedOffsets: sample.modifiedOffsets.map(BigInt) })
          changed.push(row)
        }
        if (changed.length) onRowsChanged(changed)
        // A malformed or partial response must not create an endless retry loop.
        if (changed.length < rows.length) throw new Error('The minimap sample response was incomplete.')
      } else {
        const offset = missing[0]! * BigInt(input.bytesPerRow)
        const remaining = input.fileSize - offset
        const visibleBytes = Number(input.geometry.visibleMiniRows) * input.bytesPerRow
        const length = Number(remaining < BigInt(Math.min(65_536, Math.max(input.bytesPerRow, visibleBytes)))
          ? remaining : BigInt(Math.min(65_536, Math.max(input.bytesPerRow, visibleBytes))))
        const page = await api.readPage(offset, length)
        if (disposed || ticket !== generation) return
        if (page.revision !== input.revision) throw new Error('The minimap response is no longer current.')
        addPage(page, input)
        // An accepted main viewport page may have filled this row while the
        // minimap read was in flight. Cache growth is not the success test.
        if (!cache.has(missing[0]!)) throw new Error('The minimap page did not contain the requested row.')
      }
    } catch (error) {
      if (!disposed && ticket === generation && failedGeneration !== generation) {
        failedGeneration = generation
        onError(error)
      }
    } finally {
      running = false
      if (!disposed && current?.enabled && hasCurrentPage(current) && failedGeneration !== generation) schedule()
    }
  }

  function update(input: MinimapDataInput): void {
    if (disposed) return
    if (!input.enabled) {
      generation += 1
      current = null
      key = ''
      cache.clear()
      if (scheduled) clearTimeout(scheduled)
      scheduled = null
      return
    }
    const nextKey = `${input.sourceIdentity}|${input.sourceKey}|${input.bytesPerRow}|${geometryKey(input.geometry)}`
    const identityChanged = nextKey !== key
    const revisionChanged = current?.revision !== input.revision
    const windowChanged = current?.geometry.topRow !== input.geometry.topRow ||
      current?.geometry.tileOriginSubPx !== input.geometry.tileOriginSubPx
    if (identityChanged || revisionChanged || windowChanged) {
      generation += 1
      failedGeneration = -1
      if (identityChanged || (revisionChanged && (!pendingDelta || pendingDelta.revision !== input.revision))) cache.clear()
      else if (revisionChanged && pendingDelta) {
        const row = pendingDelta.offset / BigInt(input.bytesPerRow)
        cache.delete(row)
        onRowsChanged([row])
      }
    }
    current = input
    key = nextKey
    if (pendingDelta?.revision === input.revision) pendingDelta = null
    const needed = new Set(plannedRows(input))
    for (const row of cache.keys()) if (!needed.has(row)) cache.delete(row)
    if (input.acceptedPage) addPage(input.acceptedPage, input)
    schedule()
  }

  function applyEditDelta(delta: { offset: bigint; revision: string }): void {
    if (disposed) return
    pendingDelta = delta
    if (current?.revision === delta.revision) {
      const row = delta.offset / BigInt(current.bytesPerRow)
      cache.delete(row)
      onRowsChanged([row])
      pendingDelta = null
      schedule()
    }
  }

  return {
    update,
    getRow: (row) => cache.get(row) ?? null,
    applyEditDelta,
    retry() {
      if (disposed || !current?.enabled) return
      failedGeneration = -1
      schedule()
    },
    dispose() {
      disposed = true
      generation += 1
      current = null
      cache.clear()
      if (scheduled) clearTimeout(scheduled)
      scheduled = null
    },
  }
}
