<script setup lang="ts">
import { useForm } from '@inertiajs/vue3';

const form = useForm({
    number: '',
    pin: '',
    keep: false,
});

function submit() {
    const payload = { ...form.data(), via: 'dialog' };
    form
        .transform(() => payload)
        .post('/renewals', {
            preserveState: true,
        });
}
</script>

<template>
    <form @submit.prevent="submit">
        <input v-model="form.number" type="text" />
        <input v-model="form.pin" type="password" />
    </form>
</template>
