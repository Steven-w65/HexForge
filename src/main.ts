import { createApp } from 'vue'
import './styles/theme.css'
import { startupTimings } from './startup/timings'

startupTimings.mark('entry')

// Only the selected root is fetched and evaluated. Shared dependencies remain
// cached chunks; opening the editor does not load the hex viewer a second time.
export const startup = (async () => {
  try {
    startupTimings.mark('root-load-start')
    const { default: Root } = await (new URLSearchParams(window.location.search).get('view') === 'template-editor'
      ? import('./components/TemplateEditorWindow.vue')
      : import('./App.vue'))
    startupTimings.mark('root-loaded')
    createApp(Root).mount('#app')
    startupTimings.mark('root-mounted')
    if (new URLSearchParams(window.location.search).get('view') !== 'template-editor') startupTimings.beginMainInterface()
  } catch {
    // A failed chunk must not leave an unexplained blank native window.
    const target = document.getElementById('app')
    if (!target) return
    const message = document.createElement('p')
    message.setAttribute('role', 'alert')
    message.textContent = 'HexForge could not start. Please reopen the application.'
    target.replaceChildren(message)
  }
})()
