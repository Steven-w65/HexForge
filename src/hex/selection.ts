export interface ByteSelection {
  start: bigint
  end: bigint
  count: bigint
}

export function normalizeSelection(anchor: bigint, focus: bigint): ByteSelection {
  const start = anchor <= focus ? anchor : focus
  const end = anchor <= focus ? focus : anchor
  return { start, end, count: end - start + 1n }
}

export function containsOffset(selection: ByteSelection | null | undefined, offset: bigint): boolean {
  return selection !== null && selection !== undefined && offset >= selection.start && offset <= selection.end
}
