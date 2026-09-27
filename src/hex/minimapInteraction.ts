import { ROW_HEIGHT } from './layout'
import { MINIMAP_SUBPIXELS, rowAtY, type MinimapGeometry } from './minimapGeometry'

export interface MinimapInteractionBindings {
  geometry: () => MinimapGeometry
  visibleMainRows: () => bigint
  getMainRow: () => bigint
  setMainRow: (row: bigint) => void
  setVisualTop: (topPx: number | null) => void
}

export interface MinimapInteractionController {
  click(y: number): void
  beginViewportDrag(y: number, visibleStart: bigint): void
  dragViewport(y: number): void
  wheel(deltaY: number): void
  endDrag(): void
}

function clamp(value: bigint, maximum: bigint): bigint {
  return value < 0n ? 0n : value > maximum ? maximum : value
}

export function createMinimapInteractionController(bindings: MinimapInteractionBindings): MinimapInteractionController {
  let grabOffset: bigint | null = null
  let proportionalDrag: { grabOffsetPx: number; startY: number; startEditorRow: bigint } | null = null
  const mouseCanvasY = (g: MinimapGeometry, y: number): bigint =>
    g.tileOriginSubPx + BigInt(Math.floor((Number.isFinite(y) ? y : 0) * Number(MINIMAP_SUBPIXELS)))
  const mainMaximum = (g: MinimapGeometry): bigint => {
    const visible = bindings.visibleMainRows()
    return g.totalRows > visible ? g.totalRows - visible : 0n
  }
  function driveMain(row: bigint): void {
    const target = clamp(row, mainMaximum(bindings.geometry()))
    bindings.setMainRow(target)
  }

  return {
    click(y) {
      const g = bindings.geometry()
      const target = rowAtY(g, g.mode === 'proportional' ? mouseCanvasY(g, y) : y)
      // A proportional preview click targets the row under the pointer;
      // fit mode retains its existing centered-navigation behavior.
      driveMain(g.mode === 'proportional' ? target : target - bindings.visibleMainRows() / 2n)
    },
    beginViewportDrag(y, visibleStart) {
      const g = bindings.geometry()
      if (g.mode === 'proportional') {
        // The box is the drag control. Keep the exact initial grab offset;
        // row quantization must never pull the visual box off the pointer.
        proportionalDrag = {
          grabOffsetPx: y - Number(g.viewportTopSubPx) / Number(MINIMAP_SUBPIXELS),
          startY: y,
          startEditorRow: visibleStart,
        }
        return
      }
      const touched = rowAtY(g, y)
      grabOffset = touched > visibleStart ? touched - visibleStart : 0n
    },
    dragViewport(y) {
      if (proportionalDrag) {
        const g = bindings.geometry()
        if (!Number.isFinite(y)) return
        const top = Math.max(0, Math.min(g.viewportTravelPx, y - proportionalDrag.grabOffsetPx))
        bindings.setVisualTop(top)
        if (g.viewportTravelPx === 0) return
        const topSubPx = BigInt(Math.round(top * Number(MINIMAP_SUBPIXELS)))
        // Full slider travel maps to [0, N - V]. The visual top stays
        // continuous; only this main-editor row is rounded.
        driveMain((g.maxMainRow * topSubPx + BigInt(g.viewportTravelPx) * MINIMAP_SUBPIXELS / 2n) /
          (BigInt(g.viewportTravelPx) * MINIMAP_SUBPIXELS))
        return
      }
      if (grabOffset === null) return
      driveMain(rowAtY(bindings.geometry(), y) - grabOffset)
    },
    wheel(deltaY) {
      if (!Number.isFinite(deltaY) || deltaY === 0) return
      const steps = bindings.geometry().mode === 'proportional'
        ? 1n // Match the hex canvas wheel policy; fit keeps its prior policy.
        : BigInt(Math.max(1, Math.round(Math.abs(deltaY) / ROW_HEIGHT)))
      driveMain(bindings.getMainRow() + (deltaY > 0 ? steps : -steps))
    },
    endDrag() {
      if (proportionalDrag) bindings.setVisualTop(null)
      grabOffset = null
      proportionalDrag = null
    },
  }
}
