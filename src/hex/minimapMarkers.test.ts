import { describe, expect, it } from 'vitest'
import type { NavigableParsedLeaf } from '../types'
import { createMinimapGeometry } from './minimapGeometry'
import { collectMinimapMarkers, projectMinimapMarkers, projectOverviewMarkers } from './minimapMarkers'

const field = (offset: bigint, length: number): NavigableParsedLeaf => ({
  name: 'Field', path: 'Field', offset: offset.toString(), length, type: 'bytes', value: '',
})

describe('shared minimap and overview markers', () => {
  it('maps search and template spans crossing hex-row boundaries', () => {
    const markers = collectMinimapMarkers([15n], 3, [field(31n, 3)], { binCount: 1024, bins: [] }, 64n, 16)
    expect(markers).toEqual([
      { kind: 'search', row: 0n, endRowExclusive: 2n },
      { kind: 'template', row: 1n, endRowExclusive: 3n },
    ])
  })

  it('maps the final modified overview bin to the last source row', () => {
    const markers = collectMinimapMarkers([], 1, [], { binCount: 1024, bins: [1023] }, 160n, 16)
    expect(markers).toEqual([{ kind: 'modified', row: 9n, endRowExclusive: 10n }])
    expect(projectOverviewMarkers(markers, 10n, 100)).toEqual([{ kind: 'modified', top: 99, height: 1 }])
  })

  it('projects offsets beyond JavaScript safe integers with bigint math', () => {
    const offset = (1n << 56n) + 16n
    const markers = collectMinimapMarkers([offset], 1, [], { binCount: 1024, bins: [] }, offset + 16n, 16)
    expect(markers[0]?.row).toBe(offset / 16n)
    expect(markers[0]?.endRowExclusive).toBe(offset / 16n + 1n)
  })

  it('places a last-row search marker at the bottom of the file-wide ruler', () => {
    const markers = collectMinimapMarkers([99n], 1, [], { binCount: 1024, bins: [] }, 100n, 16)
    expect(projectOverviewMarkers(markers, 7n, 500)).toEqual([{ kind: 'search', top: 499, height: 1 }])
  })

  it('coalesces 10,000 dense matches into at most one run per visible pixel', () => {
    const markers = collectMinimapMarkers(Array.from({ length: 10_000 }, (_, index) => BigInt(index * 16)), 1, [],
      { binCount: 1024, bins: [] }, 160_000n, 16)
    const geometry = createMinimapGeometry({ mode: 'fit', totalRows: 10_000n, heightPx: 500, rowPx: 2, topRow: 0n })
    const runs = projectMinimapMarkers(markers, geometry)
    expect(runs.length).toBeLessThanOrEqual(500)
    expect(runs[0]).toMatchObject({ kind: 'search', top: 0 })
  })

  it('uses modified over template over search for colliding pixel runs', () => {
    const markers = collectMinimapMarkers([0n], 16, [field(0n, 16)], { binCount: 1024, bins: [0] }, 160n, 16)
    const geometry = createMinimapGeometry({ mode: 'fit', totalRows: 10n, heightPx: 10, rowPx: 1, topRow: 0n })
    expect(projectMinimapMarkers(markers, geometry)[0]).toEqual({ kind: 'modified', top: 0, height: 1 })
    expect(projectOverviewMarkers(markers, 10n, 10)[0]).toEqual({ kind: 'modified', top: 0, height: 1 })
  })

  it('excludes off-window markers from proportional preview but keeps them in the overview ruler', () => {
    const markers = collectMinimapMarkers([0n, 1000n * 16n], 1, [], { binCount: 1024, bins: [] }, 160_000n, 16)
    const geometry = createMinimapGeometry({ mode: 'proportional', totalRows: 10_000n, heightPx: 100, rowPx: 2, topRow: 995n })
    const minimap = projectMinimapMarkers(markers, geometry)
    const overview = projectOverviewMarkers(markers, 10_000n, 100)
    expect(minimap).toEqual([{ kind: 'search', top: 10, height: 2 }])
    expect(overview.some((run) => run.top === 0)).toBe(true)
    expect(overview.some((run) => run.top > 0)).toBe(true)
  })

  it('projects a proportional marker exactly after a huge canvas-local origin', () => {
    const top = (1n << 56n) + 17n
    const geometry = createMinimapGeometry({ mode: 'proportional', totalRows: top + 100n, heightPx: 100, rowPx: 2, topRow: top })
    expect(projectMinimapMarkers([{ kind: 'search', row: top + 3n, endRowExclusive: top + 4n }], geometry))
      .toEqual([{ kind: 'search', top: 6, height: 2 }])
  })

  it('clips proportional markers against a fractional tile origin', () => {
    const g = createMinimapGeometry({ mode: 'proportional', totalRows: 1000n, heightPx: 101,
      rowPx: 2, topRow: 495n, visibleMainRows: 10n, visualTopPx: 40.25 })
    const first = g.topRow
    const result = projectMinimapMarkers([{ kind: 'search', row: first, endRowExclusive: first + 1n }], g)
    expect(result).toHaveLength(1)
    expect(result[0]!.top).toBe(0)
    expect(result[0]!.height).toBeLessThan(2)
  })

  it('rasterizes a marker beginning between proportional tile pixels', () => {
    const g = createMinimapGeometry({ mode: 'proportional', totalRows: 1000n, heightPx: 101,
      rowPx: 2, topRow: 495n, visibleMainRows: 10n, visualTopPx: 40.25 })
    const row = g.topRow + 1n
    expect(projectMinimapMarkers([{ kind: 'search', row, endRowExclusive: row + 1n }], g))
      .toEqual([{ kind: 'search', top: 0, height: 3 }])
  })
})
