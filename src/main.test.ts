import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'

const loaded: string[] = []

describe('window bootstrap', () => {
  beforeEach(() => {
    vi.resetModules()
    loaded.length = 0
    document.body.innerHTML = '<div id="app"></div>'
    vi.doMock('./App.vue', async () => {
      loaded.push('main')
      const { defineComponent, h } = await import('vue')
      return { default: defineComponent({ render: () => h('main', 'Hex viewer') }) }
    })
    vi.doMock('./components/TemplateEditorWindow.vue', async () => {
      loaded.push('editor')
      const { defineComponent, h } = await import('vue')
      return { default: defineComponent({ render: () => h('main', 'Template Editor') }) }
    })
  })
  afterEach(() => { window.history.replaceState(null, '', '/') })

  it('mounts the main viewer without evaluating the Template Editor', async () => {
    window.history.replaceState(null, '', '/')
    await import('./main')
    await flushPromises()
    expect(document.querySelector('#app')?.textContent).toBe('Hex viewer')
    expect(loaded).toEqual(['main'])
  })

  it('mounts the editor without evaluating the main viewer', async () => {
    window.history.replaceState(null, '', '/?view=template-editor')
    await import('./main')
    await flushPromises()
    expect(document.querySelector('#app')?.textContent).toBe('Template Editor')
    expect(loaded).toEqual(['editor'])
  })
})
