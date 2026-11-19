<script setup lang="ts">
import { inject } from 'vue';
import { ThemeKey } from '../keys';

defineProps<{ label: string }>();

const open = defineModel<boolean>('open');
const text = defineModel<string>({ required: true });
const theme = inject(ThemeKey);
const locale = inject('locale', 'en');
</script>

<template>
    <section :data-theme="theme" :lang="locale">
        <header><slot name="head" /></header>
        <input v-model="text" />
        <button type="button" @click="open = !open">toggle</button>
        <slot />
    </section>
</template>
