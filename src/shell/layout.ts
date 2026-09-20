export const COMPACT_LEFT_WIDTH = 200
export const COMPACT_RIGHT_WIDTH = 260

export function compactCenterWidth(windowWidth: number): number {
  return windowWidth - COMPACT_LEFT_WIDTH - COMPACT_RIGHT_WIDTH
}
