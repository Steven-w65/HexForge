<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { lockModalFocus } from '../ui/modalFocus'

const props = defineProps<{ open: boolean; title?: string; message?: string }>()
defineEmits<{ close: [] }>()
const dialog = ref<HTMLElement | null>(null)
const backdrop = ref<HTMLElement | null>(null)
let release: (() => void) | undefined
let generation = 0

async function focusInitialControl(): Promise<void> {
  if (!props.open) return
  const current = ++generation
  const invoking = document.activeElement instanceof HTMLElement ? document.activeElement : null
  await nextTick()
  if (!props.open || current !== generation || !backdrop.value) return
  release?.()
  release = lockModalFocus(backdrop.value, invoking)
}

watch(() => props.open, (open) => {
  if (open) void focusInitialControl()
  else { generation += 1; release?.(); release = undefined }
})
onMounted(() => { void focusInitialControl() })
onBeforeUnmount(() => { generation += 1; release?.() })
</script>

<template>
  <div v-if="open" ref="backdrop" class="dialog-backdrop" role="presentation" tabindex="-1" @click.self="$emit('close')">
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
header strong { font-size: var(--font-heading); }
button { width: 26px; height: 26px; color: var(--muted); background: transparent; border: 0; border-radius: 4px; font: inherit; font-size: 18px; }
button:hover, button:focus-visible { color: var(--text); background: var(--hover); outline: none; }
p { margin: 10px 0 0; color: var(--muted); font-size: var(--font-body); line-height: 1.5; }
</style>
