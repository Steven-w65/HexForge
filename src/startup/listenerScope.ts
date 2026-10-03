type Unlisten = () => void

/** Own concurrent registrations, including ones that resolve after teardown. */
export function createListenerScope({ disposeOnError = true } = {}) {
  const disposers: Unlisten[] = []
  let disposed = false

  function own(unlisten: Unlisten): void {
    if (disposed) unlisten()
    else disposers.push(unlisten)
  }

  function dispose(): void {
    disposed = true
    disposers.splice(0).forEach(unlisten => unlisten())
  }

  async function register(registrations: Array<() => Promise<Unlisten>>): Promise<void> {
    if (disposed) throw new Error('Listener registration cancelled.')
    try {
      // Each factory starts immediately; do not wait for one IPC before the next.
      await Promise.all(registrations.map(async register => { own(await register()) }))
      if (disposed) throw new Error('Listener registration cancelled.')
    } catch (error) {
      // Atomic bridge registrations roll back on failure. Main-window safety
      // listeners instead remain owned until teardown: losing drop support
      // must never remove an already installed unsaved-change close guard.
      if (disposeOnError) dispose()
      throw error
    }
  }

  return { own, register, dispose, isDisposed: () => disposed }
}
