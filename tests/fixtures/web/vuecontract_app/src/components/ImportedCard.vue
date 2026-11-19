<script setup lang="ts">
import type { CardProps } from '../types/card';

const props = defineProps<CardProps>();
</script>

<template>
    <article>{{ props.heading }}</article>
</template>
