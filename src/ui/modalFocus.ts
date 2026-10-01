/** One stack per webview: a nested error dialog must not release its parent's lock. */
const stack: Array<{ root: HTMLElement; returnFocus: HTMLElement | null }> = []
const changed = new Map<HTMLElement, boolean>()
let observer: MutationObserver | null = null

/** Inert blocks pointer/focus access, but window-level shortcuts need this guard too. */
export function isModalOpen(): boolean { return stack.length > 0 }

function focusable(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>('button, input, select, textarea, a[href], [tabindex]')]
    .filter(element => element.tabIndex >= 0 && !element.matches(':disabled') && !element.closest('[hidden], [inert]'))
}
function focusFirst(root: HTMLElement): void {
  const target = root.querySelector<HTMLElement>('[autofocus]:not(:disabled)') ?? focusable(root)[0] ?? root
  target.focus()
}
function updateInert(): void {
  for (const [element, wasInert] of changed) element.toggleAttribute('inert', wasInert)
  changed.clear()
  let branch: HTMLElement | null = stack.at(-1)?.root ?? null
  while (branch?.parentElement) {
    for (const sibling of branch.parentElement.children) {
      if (sibling === branch || !(sibling instanceof HTMLElement)) continue
      changed.set(sibling, sibling.hasAttribute('inert'))
      sibling.setAttribute('inert', '')
    }
    branch = branch.parentElement
    if (branch === document.body) break
  }
}
function keepFocus(event: FocusEvent): void {
  const root = stack.at(-1)?.root
  if (root && event.target instanceof Node && !root.contains(event.target)) focusFirst(root)
}
function trapTab(event: KeyboardEvent): void {
  const root = stack.at(-1)?.root
  if (!root || event.key !== 'Tab') return
  const controls = focusable(root)
  const first = controls[0], last = controls.at(-1)
  if (!first || !last) { event.preventDefault(); root.focus(); return }
  if (event.shiftKey && (document.activeElement === first || !root.contains(document.activeElement))) {
    event.preventDefault(); last.focus()
  } else if (!event.shiftKey && (document.activeElement === last || !root.contains(document.activeElement))) {
    event.preventDefault(); first.focus()
  }
}

export function lockModalFocus(root: HTMLElement, returnFocus: HTMLElement | null): () => void {
  const entry = { root, returnFocus }
  stack.push(entry)
  if (stack.length === 1) {
    document.addEventListener('focusin', keepFocus, true)
    document.addEventListener('keydown', trapTab, true)
    observer = new MutationObserver(updateInert)
    observer.observe(document.body, { childList: true, subtree: true })
  }
  updateInert(); focusFirst(root)
  return () => {
    const wasTop = stack.at(-1) === entry
    const index = stack.indexOf(entry)
    if (index < 0) return
    stack.splice(index, 1); updateInert()
    if (!stack.length) {
      observer?.disconnect(); observer = null
      document.removeEventListener('focusin', keepFocus, true)
      document.removeEventListener('keydown', trapTab, true)
    }
    if (wasTop) {
      const parent = stack.at(-1)?.root
      if (returnFocus?.isConnected && !returnFocus.closest('[inert]')) returnFocus.focus()
      else if (parent) focusFirst(parent)
    }
  }
}
