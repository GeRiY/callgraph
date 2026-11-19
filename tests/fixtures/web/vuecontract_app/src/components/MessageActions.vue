<script setup lang="ts">
defineProps<{ title: string; count?: number; tone: 'soft' | 'loud' }>();

const emit = defineEmits<{
    (e: 'purge', id: string): void;
    (e: 'pin'): void;
}>();

function onSelect(id: string): void {
    emit('purge', id);
}

defineExpose({ onSelect });
</script>

<template>
    <div class="actions">
        <button type="button" @click="onSelect('x')">x</button>
        <slot name="footer" />
        <slot />
    </div>
</template>
