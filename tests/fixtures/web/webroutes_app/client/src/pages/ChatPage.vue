<script setup lang="ts">
import { loadChat, sendMessage } from '../composables/api';

async function send(id: string, text: string) {
    await sendMessage(id, text);
    return loadChat(id);
}
</script>

<template>
    <button @click="send('1', 'hi')">Send</button>
</template>
