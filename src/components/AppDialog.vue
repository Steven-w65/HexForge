<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from 'vue'

const props = defineProps<{ open: boolean; title?: string; message?: string }>()
defineEmits<{ close: [] }>()
const dialog = ref<HTMLElement | null>(null)

async function focusInitialControl(): Promise<void> {
  if (!props.open) return
  await nextTick()
  const target = dialog.value?.querySelector<HTMLElement>('[autofocus]') ??
    dialog.value?.querySelector<HTMLElement>('input, select, textarea, button')
  target?.focus()
}

watch(() => props.open, (open) => { if (open) void focusInitialControl() })
onMounted(() => { void focusInitialControl() })
</script>

<template>
  <div v-if="open" class="dialog-backdrop" role="presentation" @click.self="$emit('close')">
    <section ref="dialog" class="dialog" role="dialog" aria-modal="true" :aria-label="title ?? 'HexForge dialog'" @keydown.esc.stop.prevent="$emit('close')">
      <header><strong>{{ title }}</strong><button type="button" aria-label="Close dialog" @click="$emit('close')">×</button></header>
      <p v-if="message">{{ message }}</p>
      <slot />
    </section>
  </div>
</template>

<style scoped>
.dialog-backdrop { position: fixed; z-index: 10; inset: 0; display: grid; place-items: center; padding: 20px; background: rgb(0 0 0 / 42%); }
.dialog { box-sizing: border-box; width: min(420px, 100%); min-width: 280px; max-width: 560px; padding: 15px; color: var(--text); background: var(--panel); border: 1px solid var(--border-strong); border-radius: 7px; box-shadow: 0 8px 24px rgb(0 0 0 / 16%); }
header { display: flex; align-items: center; justify-content: space-between; }
header strong { font-size: 12px; }
button { width: 26px; height: 26px; color: var(--muted); background: transparent; border: 0; border-radius: 4px; font: inherit; font-size: 18px; }
button:hover, button:focus-visible { color: var(--text); background: var(--hover); outline: none; }
p { margin: 10px 0 0; color: var(--muted); font-size: 10px; line-height: 1.5; }
</style>
