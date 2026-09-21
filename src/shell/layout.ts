export const COMPACT_LEFT_WIDTH = 200
export const COMPACT_RIGHT_WIDTH = 260
export const TOOLBAR_COMPACT_ACTION_WIDTH = 31
export const TOOLBAR_FIXED_WIDTH = 160

export function compactCenterWidth(windowWidth: number): number {
  return windowWidth - COMPACT_LEFT_WIDTH - COMPACT_RIGHT_WIDTH
}

export function compactToolbarRequiredWidth(actionCount: number): number {
  return TOOLBAR_FIXED_WIDTH + actionCount * TOOLBAR_COMPACT_ACTION_WIDTH
}
