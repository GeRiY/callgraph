<script setup lang="ts">
defineProps<{ label: string }>();
const emit = defineEmits<{ (e: 'stamp'): void }>();
</script>

<template>
    <button type="button" @click="emit('stamp')">{{ label }}</button>
</template>
