<script setup lang="ts">
import { useRouter } from 'vue-router';
import { listChats } from '../composables/api';

const router = useRouter();

async function load() {
    return listChats();
}

function openSettings() {
    router.push({ name: 'settings' });
}
</script>

<template>
    <button @click="load">Reload</button>
    <router-link :to="{ name: 'home' }">Home</router-link>
    <router-link :to="{ name: 'archive' }">Archive</router-link>
    <button @click="openSettings">Settings</button>
</template>
