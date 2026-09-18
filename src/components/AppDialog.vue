<script setup lang="ts">
defineProps<{ open: boolean; title?: string; message?: string }>()
defineEmits<{ close: [] }>()
</script>

<template>
  <div v-if="open" class="dialog-backdrop" role="presentation" @click.self="$emit('close')">
    <section class="dialog" role="dialog" aria-modal="true" :aria-label="title ?? 'HexForge dialog'">
      <header><strong>{{ title }}</strong><button type="button" aria-label="Close dialog" @click="$emit('close')">×</button></header>
      <p v-if="message">{{ message }}</p>
      <slot />
    </section>
  </div>
</template>

<style scoped>
.dialog-backdrop { position: fixed; z-index: 10; inset: 0; display: grid; place-items: center; background: rgb(0 0 0 / 45%); }
.dialog { min-width: 320px; max-width: min(560px, calc(100vw - 40px)); padding: 14px; color: var(--text); background: var(--panel); border: 1px solid var(--border); border-radius: 6px; box-shadow: 0 12px 30px rgb(0 0 0 / 18%); }
header { display: flex; align-items: center; justify-content: space-between; }
button { color: var(--muted); background: transparent; border: 0; font: inherit; font-size: 18px; }
p { color: var(--muted); }
</style>
