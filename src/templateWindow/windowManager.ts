import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { EDITOR_LABEL, errorMessage } from './protocol'

interface WindowHandle {
  show(): Promise<void>
  unminimize(): Promise<void>
  setFocus(): Promise<void>
  close(): Promise<void>
  destroy(): Promise<void>
}

interface WindowFactory { find(): Promise<WindowHandle | null>; create(): Promise<WindowHandle> }

export function createWindowManager(factory: WindowFactory) {
  let current: WindowHandle | null = null
  let opening: Promise<void> | null = null

  async function openOrFocus(): Promise<void> {
    if (opening) return opening
    opening = (async () => {
      try {
        current = current ?? await factory.find() ?? await factory.create()
        await current.show()
        await current.unminimize()
        await current.setFocus()
      } catch (error) { current = null; throw error }
      finally { opening = null }
    })()
    return opening
  }

  async function close(): Promise<void> {
    const window = current ?? await factory.find()
    if (window) await window.close()
    current = null
  }

  async function destroy(): Promise<void> {
    if (opening) await opening.catch(() => undefined)
    const window = await factory.find()
    if (window) await window.destroy()
    current = null
  }

  function forget(): void { current = null }
  return { openOrFocus, close, destroy, forget }
}

async function createNativeWindow(): Promise<WebviewWindow> {
  const window = new WebviewWindow(EDITOR_LABEL, {
    url: '/?view=template-editor', title: 'HexForge — Template Editor', width: 660, height: 760,
    minWidth: 480, minHeight: 440, center: true, resizable: true,
  })
  await new Promise<void>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Template Editor could not be opened.')), 5000)
    void window.once('tauri://created', () => { clearTimeout(timer); resolve() }).catch((error) => { clearTimeout(timer); reject(error) })
    void window.once('tauri://error', (event) => { clearTimeout(timer); reject(new Error(errorMessage(event.payload))) }).catch((error) => { clearTimeout(timer); reject(error) })
  })
  return window
}

export const templateWindowManager = createWindowManager({
  find: () => WebviewWindow.getByLabel(EDITOR_LABEL),
  create: createNativeWindow,
})
