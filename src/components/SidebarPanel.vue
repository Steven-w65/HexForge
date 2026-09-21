<script setup lang="ts">
import type { FileInfo, TemplateDefinition } from '../types'
import TemplateEditor from './TemplateEditor.vue'

defineProps<{ file: FileInfo | null; template: TemplateDefinition; collapsed: boolean }>()
const emit = defineEmits<{
  'toggle-collapse': []
  'update:template': [value: TemplateDefinition]
  'save-template': []; 'load-template': []; navigate: [range: { start: bigint; end: bigint }]; 'template-validity': [valid: boolean]
}>()

function formatSize(value: string): string {
  const bytes = BigInt(value)
  if (bytes < 1024n) return `${bytes} B`
  if (bytes < 1024n ** 2n) return `${Number(bytes) / 1024} KiB`
  if (bytes < 1024n ** 3n) return `${(Number(bytes) / 1024 ** 2).toFixed(2)} MiB`
  return `${(Number(bytes) / 1024 ** 3).toFixed(2)} GiB`
}
</script>

<template>
  <aside class="sidebar-panel" :class="{ collapsed }">
    <button type="button" class="collapse" data-action="collapse-left" :title="collapsed ? 'Expand sidebar' : 'Collapse sidebar'" @click="emit('toggle-collapse')">{{ collapsed ? '›' : '‹' }}</button>
    <div v-if="!collapsed" class="panel-content">
      <section class="file-card">
        <div class="panel-heading"><span>FILE</span><span v-if="file?.dirty" class="modified">MODIFIED</span></div>
        <template v-if="file">
          <strong>{{ file.name }}</strong>
          <span class="path" :title="file.path">{{ file.path }}</span>
          <span>{{ formatSize(file.size) }} · {{ file.size }} bytes</span>
        </template>
        <span v-else class="empty">No file open</span>
      </section>
      <section class="template-panel">
        <div class="panel-heading">PARSING TEMPLATE</div>
        <TemplateEditor :model-value="template" @update:model-value="emit('update:template', $event)" @validity="emit('template-validity', $event)" @save="emit('save-template')" @load="emit('load-template')" @navigate="emit('navigate', $event)" />
      </section>
    </div>
  </aside>
</template>

<style scoped>
.sidebar-panel { position: relative; min-width: 0; min-height: 0; background: var(--panel); border-right: 1px solid var(--border); }
.sidebar-panel.collapsed { width: 28px; }
.collapse { position: absolute; z-index: 2; top: 5px; right: 4px; width: 21px; height: 21px; color: var(--muted); background: transparent; border: 0; border-radius: 3px; }
.collapse:hover { color: var(--text); background: var(--hover); }
.panel-content { box-sizing: border-box; height: 100%; display: flex; flex-direction: column; gap: 10px; padding: 8px; overflow: hidden; }
.file-card { display: grid; gap: 5px; padding: 9px; background: var(--card); border: 1px solid var(--border); border-radius: 5px; font-size: 10px; }
.panel-heading { display: flex; justify-content: space-between; padding-right: 20px; color: var(--muted); font-size: 10px; letter-spacing: .08em; }
.path { overflow: hidden; color: var(--muted); text-overflow: ellipsis; white-space: nowrap; }
.modified { color: var(--modified); }
.empty { color: var(--muted); }
.template-panel { display: flex; flex: 1; min-height: 0; flex-direction: column; gap: 7px; }
</style>
