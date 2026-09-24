<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import type { BytesPerRow } from '../hex/layout'
import { commandEnabled, type MenuCommand, type MenuState } from '../menu/commands'
import type { ParsedField, TemplateDefinition, TemplateField } from '../types'

type MenuId = 'file' | 'edit' | 'navigate' | 'template' | 'view'
type SubmenuId = 'template-fields' | 'parsed-results' | null
type VisibleMenuCommand = Exclude<MenuCommand, 'toggle-right-panel'>

const props = defineProps<{
  state: MenuState
  bytesPerRow: BytesPerRow
  fileSize: bigint
  template: TemplateDefinition
  results: ParsedField[]
}>()
const emit = defineEmits<{
  command: [value: MenuCommand]
  navigate: [range: { start: bigint; end: bigint }]
}>()

const root = ref<HTMLElement | null>(null)
const openMenu = ref<MenuId | null>(null)
const openSubmenu = ref<SubmenuId>(null)
const popupLeft = ref(8)
const submenuTop = ref(0)
const menuOrder: MenuId[] = ['file', 'edit', 'navigate', 'template', 'view']
const accessKeys: Record<string, MenuId> = { f: 'file', e: 'edit', n: 'navigate', t: 'template', v: 'view' }

const labels: Record<VisibleMenuCommand, string> = {
  open: 'Open File…', 'close-file': 'Close File', 'save-as': 'Save As…', export: 'Export Parsed Results as CSV…', exit: 'Exit',
  'edit-selected': 'Edit Selected Byte…', 'toggle-edit': 'Edit Mode', undo: 'Undo Byte Edit',
  goto: 'Go To Offset…', search: 'Search Bytes…',
  'template-editor': 'Template Editor…', 'apply-template': 'Apply Template', 'load-template': 'Load Template…', 'save-template': 'Save Template As…', 'add-field': 'Add Field',
  'theme-toggle': 'Toggle Theme',
  'row-16': '16 Bytes', 'row-32': '32 Bytes',
}
const shortcutLabels: Partial<Record<VisibleMenuCommand, string>> = {
  open: 'Ctrl+O', 'close-file': 'Ctrl+W', 'save-as': 'Ctrl+Shift+S', export: 'Ctrl+Shift+E', exit: 'Alt+F4',
  'edit-selected': 'F2', 'toggle-edit': 'Ctrl+Alt+E', undo: 'Ctrl+Z', goto: 'Ctrl+G', search: 'Ctrl+F',
  'template-editor': 'Ctrl+Shift+T', 'apply-template': 'Ctrl+Enter', 'load-template': 'Ctrl+Alt+O', 'save-template': 'Ctrl+S', 'add-field': 'Ctrl+Alt+A',
  'theme-toggle': 'Ctrl+Alt+T',
  'row-16': 'Ctrl+1', 'row-32': 'Ctrl+2',
}

const fileCommands: VisibleMenuCommand[] = ['open', 'close-file', 'save-as', 'export', 'exit']
const editCommands: VisibleMenuCommand[] = ['edit-selected', 'toggle-edit', 'undo']
const navigateCommands: VisibleMenuCommand[] = ['goto', 'search']
const templateCommands: VisibleMenuCommand[] = ['template-editor', 'apply-template', 'load-template', 'save-template', 'add-field']
const viewCommands: VisibleMenuCommand[] = ['theme-toggle', 'row-16', 'row-32']

function anchorMenu(menu: MenuId, button?: EventTarget | null): void {
  const anchor = button instanceof HTMLElement ? button : root.value?.querySelector<HTMLElement>(`[data-menu="${menu}"]`)
  popupLeft.value = anchor?.offsetLeft ?? 8
}

function toggleMenu(menu: MenuId, button: EventTarget | null): void {
  if (openMenu.value !== menu) anchorMenu(menu, button)
  openMenu.value = openMenu.value === menu ? null : menu
  openSubmenu.value = null
}

function closeMenus(): void { openMenu.value = null; openSubmenu.value = null }

function showSubmenu(submenu: Exclude<SubmenuId, null>, button?: EventTarget | null): void {
  const trigger = button instanceof HTMLElement ? button : root.value?.querySelector<HTMLElement>(`[data-submenu="${submenu}"]`)
  submenuTop.value = trigger?.offsetTop ?? 0
  openSubmenu.value = submenu
  focusSubmenuItem(submenu === 'template-fields' ? '[data-template-field]' : '[data-parsed-result]')
}

function toggleSubmenu(submenu: Exclude<SubmenuId, null>, button: EventTarget | null): void {
  if (openSubmenu.value === submenu) { openSubmenu.value = null; return }
  showSubmenu(submenu, button)
}

function invoke(command: MenuCommand): void {
  if (!commandEnabled(command, props.state)) return
  emit('command', command)
  closeMenus()
}

function checked(command: MenuCommand): boolean | undefined {
  switch (command) {
    case 'toggle-edit': return props.state.editMode
    case 'row-16': return props.bytesPerRow === 16
    case 'row-32': return props.bytesPerRow === 32
    default: return undefined
  }
}

function fieldLength(field: TemplateField): number | null {
  if (field.type === 'string' || field.type === 'bytes') return Number.isSafeInteger(field.length) && (field.length ?? 0) > 0 ? field.length! : null
  if (field.type.endsWith('8')) return 1
  if (field.type.endsWith('16')) return 2
  if (field.type.endsWith('32') || field.type === 'f32') return 4
  return 8
}

function fieldRange(field: TemplateField): { start: bigint; end: bigint } | null {
  if (!/^(?:0[xX][0-9a-fA-F]+|[0-9]+)$/.test(field.offset.trim())) return null
  const length = fieldLength(field)
  if (length === null) return null
  try {
    const start = BigInt(field.offset)
    const end = start + BigInt(length) - 1n
    return end < props.fileSize ? { start, end } : null
  } catch { return null }
}

const navigableFields = computed(() => props.template.fields.map((field, index) => ({ field, index, range: fieldRange(field) })).filter((entry) => entry.range !== null))

function navigate(range: { start: bigint; end: bigint }): void { emit('navigate', range); closeMenus() }
function resultRange(result: ParsedField): { start: bigint; end: bigint } {
  const start = BigInt(result.offset)
  return { start, end: start + BigInt(result.length) - 1n }
}
function offsetLabel(offset: string): string {
  try { return `0x${BigInt(offset).toString(16).toUpperCase().padStart(8, '0')}` } catch { return offset }
}

function focusFirstItem(): void {
  void nextTick(() => root.value?.querySelector<HTMLElement>('.menu-popup [role^="menuitem"]:not(:disabled)')?.focus())
}

function focusSubmenuItem(selector: string): void {
  void nextTick(() => root.value?.querySelector<HTMLElement>(selector)?.focus())
}

function moveItemFocus(delta: number): void {
  const active = document.activeElement as HTMLElement | null
  const scope = active?.closest<HTMLElement>('.submenu-flyout') ?? root.value?.querySelector<HTMLElement>('.menu-popup')
  const items = [...(scope?.querySelectorAll<HTMLElement>(':scope > [role^="menuitem"]:not(:disabled)') ?? [])]
  if (items.length === 0) return
  const current = items.indexOf(active as HTMLElement)
  items[(current + delta + items.length) % items.length]!.focus()
}

function onWindowKeydown(event: KeyboardEvent): void {
  if (event.altKey && !event.ctrlKey && !event.shiftKey && !event.metaKey) {
    const menu = accessKeys[event.key.toLowerCase()]
    if (menu) { event.preventDefault(); anchorMenu(menu); openMenu.value = menu; openSubmenu.value = null; focusFirstItem(); return }
  }
  if (!openMenu.value) return
  if (event.key === 'Escape') { event.preventDefault(); event.stopImmediatePropagation(); closeMenus(); return }
  if (openMenu.value === 'navigate' && !event.altKey && !event.ctrlKey && !event.shiftKey && !event.metaKey) {
    const key = event.key.toLowerCase()
    if (key === 't' && navigableFields.value.length > 0) {
      event.preventDefault(); showSubmenu('template-fields'); return
    }
    if (key === 'r' && props.results.length > 0) {
      event.preventDefault(); showSubmenu('parsed-results'); return
    }
  }
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault(); moveItemFocus(event.key === 'ArrowDown' ? 1 : -1); return
  }
  if ((event.key === 'Enter' || event.key === ' ') && root.value?.contains(document.activeElement)) {
    const active = document.activeElement
    if (active instanceof HTMLButtonElement && active.closest('.menu-popup')) {
      event.preventDefault(); active.click(); return
    }
  }
  if (event.key === 'ArrowRight') {
    const submenu = (document.activeElement as HTMLElement | null)?.dataset.submenu as Exclude<SubmenuId, null> | undefined
    if (submenu) { event.preventDefault(); showSubmenu(submenu, document.activeElement); return }
    if (openSubmenu.value) return
  }
  if (event.key === 'ArrowLeft' && openSubmenu.value) {
    event.preventDefault()
    const submenu = openSubmenu.value
    openSubmenu.value = null
    void nextTick(() => root.value?.querySelector<HTMLElement>(`[data-submenu="${submenu}"]`)?.focus())
    return
  }
  if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
    event.preventDefault()
    const current = menuOrder.indexOf(openMenu.value)
    const delta = event.key === 'ArrowRight' ? 1 : -1
    const nextMenu = menuOrder[(current + delta + menuOrder.length) % menuOrder.length]!
    anchorMenu(nextMenu)
    openMenu.value = nextMenu
    openSubmenu.value = null
    focusFirstItem()
  }
}

function onDocumentPointer(event: MouseEvent): void {
  if (root.value && !root.value.contains(event.target as Node)) closeMenus()
}

onMounted(() => {
  window.addEventListener('keydown', onWindowKeydown, true)
  document.addEventListener('click', onDocumentPointer)
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onWindowKeydown, true)
  document.removeEventListener('click', onDocumentPointer)
})
</script>

<template>
  <nav ref="root" class="top-menu" role="menubar" aria-label="Application menu">
    <div class="menu-brand"><span class="brand-mark">HF</span><strong>HexForge</strong></div>
    <button v-for="menu in menuOrder" :key="menu" type="button" role="menuitem" :data-menu="menu"
      :aria-haspopup="true" :aria-expanded="openMenu === menu" @click.stop="toggleMenu(menu, $event.currentTarget)">
      {{ menu[0]!.toUpperCase() + menu.slice(1) }}
    </button>

    <div v-if="openMenu" class="menu-popup" role="menu" :data-open-menu="openMenu" :style="{ left: `${popupLeft}px` }" @click.stop>
      <template v-if="openMenu === 'file'">
        <MenuCommandItem v-for="command in fileCommands" :key="command" :command="command" :label="labels[command]" :shortcut="shortcutLabels[command]"
          :disabled="!commandEnabled(command, state)" @invoke="invoke" />
      </template>
      <template v-else-if="openMenu === 'edit'">
        <MenuCommandItem v-for="command in editCommands" :key="command" :command="command" :label="labels[command]" :shortcut="shortcutLabels[command]"
          :disabled="!commandEnabled(command, state)" :checked="checked(command)" @invoke="invoke" />
      </template>
      <template v-else-if="openMenu === 'navigate'">
        <MenuCommandItem v-for="command in navigateCommands" :key="command" :command="command" :label="labels[command]" :shortcut="shortcutLabels[command]"
          :disabled="!commandEnabled(command, state)" @invoke="invoke" />
        <div class="separator" />
        <button type="button" role="menuitem" class="menu-item submenu-trigger" data-submenu="template-fields"
          :aria-expanded="openSubmenu === 'template-fields'" :disabled="!state.hasFile || navigableFields.length === 0" @click="toggleSubmenu('template-fields', $event.currentTarget)">
          <span>Template Fields</span><span>›</span>
        </button>
        <div v-if="openSubmenu === 'template-fields'" class="submenu-flyout" role="menu" data-open-submenu="template-fields" :style="{ top: `${submenuTop}px` }">
          <button v-for="entry in navigableFields" :key="entry.index" type="button" role="menuitem" class="menu-item"
            :data-template-field="entry.index" @click="navigate(entry.range!)">
            <span>{{ entry.field.name }}</span><kbd>{{ offsetLabel(entry.field.offset) }}</kbd>
          </button>
        </div>
        <button type="button" role="menuitem" class="menu-item submenu-trigger" data-submenu="parsed-results"
          :aria-expanded="openSubmenu === 'parsed-results'" :disabled="!state.hasFile || results.length === 0" @click="toggleSubmenu('parsed-results', $event.currentTarget)">
          <span>Parsed Results</span><span>›</span>
        </button>
        <div v-if="openSubmenu === 'parsed-results'" class="submenu-flyout" role="menu" data-open-submenu="parsed-results" :style="{ top: `${submenuTop}px` }">
          <button v-for="(result, index) in results" :key="`${result.offset}-${result.name}-${index}`" type="button" role="menuitem" class="menu-item"
            :data-parsed-result="index" @click="navigate(resultRange(result))">
            <span>{{ result.name }}</span><kbd>{{ offsetLabel(result.offset) }}</kbd>
          </button>
        </div>
      </template>
      <template v-else-if="openMenu === 'template'">
        <MenuCommandItem v-for="command in templateCommands" :key="command" :command="command" :label="labels[command]" :shortcut="shortcutLabels[command]"
          :disabled="!commandEnabled(command, state)" @invoke="invoke" />
      </template>
      <template v-else>
        <MenuCommandItem v-for="command in viewCommands" :key="command" :command="command" :label="labels[command]" :shortcut="shortcutLabels[command]"
          :disabled="!commandEnabled(command, state)" :checked="checked(command)" @invoke="invoke" />
      </template>
    </div>
  </nav>
</template>

<script lang="ts">
import { defineComponent, h, type PropType } from 'vue'
import type { MenuCommand as Command } from '../menu/commands'

const MenuCommandItem = defineComponent({
  name: 'MenuCommandItem',
  props: {
    command: { type: String as PropType<Command>, required: true },
    label: { type: String, required: true },
    shortcut: String,
    disabled: Boolean,
    checked: { type: Boolean, default: undefined },
  },
  emits: { invoke: (_command: Command) => true },
  setup(props, { emit }) {
    return () => h('button', {
      type: 'button',
      role: props.checked === undefined ? 'menuitem' : 'menuitemcheckbox',
      class: 'menu-item',
      'data-menu-command': props.command,
      disabled: props.disabled,
      'aria-checked': props.checked === undefined ? undefined : String(props.checked),
      onClick: () => { if (!props.disabled) emit('invoke', props.command) },
    }, [
      h('span', { class: 'checkmark' }, props.checked === undefined ? '' : props.checked ? '✓' : ''),
      h('span', { class: 'item-label' }, props.label),
      props.shortcut ? h('kbd', props.shortcut) : null,
    ])
  },
})

export default { components: { MenuCommandItem } }
</script>

<style scoped>
.top-menu { position: relative; display: flex; align-items: center; height: var(--top-menu-height, 34px); padding: 0 10px; background: var(--surface); border-bottom: 1px solid var(--border); }
.menu-brand { display: flex; align-items: center; gap: 8px; margin-right: 19px; white-space: nowrap; font-size: var(--font-title); letter-spacing: .02em; }
.brand-mark { display: grid; place-items: center; width: 23px; height: 23px; color: var(--selection-text); background: var(--selection); border-radius: 4px; font-size: 10px; font-weight: 700; }
.top-menu > button { height: 29px; padding: 0 11px; color: var(--muted); background: transparent; border: 0; border-radius: 3px; font: inherit; font-size: var(--font-heading); text-transform: none; }
.top-menu > button:hover, .top-menu > button[aria-expanded='true'] { color: var(--text); background: var(--hover); }
.menu-popup { position: absolute; z-index: 20; top: calc(var(--top-menu-height, 34px) - 1px); display: grid; min-width: 280px; padding: 4px; color: var(--text); background: var(--panel); border: 1px solid var(--border-strong); border-radius: 4px; box-shadow: 0 8px 20px rgb(0 0 0 / 24%); }
.menu-item { display: grid; grid-template-columns: 14px minmax(120px, 1fr) auto; align-items: center; min-height: 33px; gap: 7px; padding: 0 9px; color: var(--text); background: transparent; border: 0; border-radius: 3px; font: inherit; font-size: var(--font-body); text-align: left; }
.menu-item:hover:not(:disabled), .menu-item:focus-visible { background: var(--hover); outline: none; }
.menu-item:disabled { opacity: .38; }
.submenu-trigger { grid-template-columns: minmax(120px, 1fr) auto; }
kbd { color: var(--muted); font: inherit; font-size: var(--font-support); }
.separator { height: 1px; margin: 4px 3px; background: var(--border); }
.submenu-flyout { position: absolute; left: calc(100% + 4px); display: grid; min-width: 230px; max-height: 280px; padding: 4px; overflow: auto; color: var(--text); background: var(--panel); border: 1px solid var(--border-strong); border-radius: 4px; box-shadow: 0 8px 20px rgb(0 0 0 / 24%); }
.submenu-flyout .menu-item { grid-template-columns: minmax(120px, 1fr) auto; }
</style>
