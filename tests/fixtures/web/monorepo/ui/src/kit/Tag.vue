<script setup lang="ts">
defineProps<{ text: string }>();
</script>

<template>
    <span class="tag">{{ text }}</span>
</template>
