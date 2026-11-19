<script setup lang="ts">
import { useRouter } from 'vue-router';

const router = useRouter();

function openFirst() {
    router.push({ name: 'chat', params: { id: '1' } });
}
</script>

<template>
    <button @click="openFirst">Open</button>
    <RouterLink :to="{ name: 'chat-list' }">All chats</RouterLink>
</template>
