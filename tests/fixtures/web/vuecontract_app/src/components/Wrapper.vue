<script setup lang="ts">
defineProps<{ a: string }>();
</script>

<template>
    <div v-bind="$attrs"><slot /></div>
</template>
