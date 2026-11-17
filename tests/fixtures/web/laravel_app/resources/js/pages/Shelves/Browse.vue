<script setup lang="ts">
import { computed } from 'vue';
import Stamp from '../../components/Stamp.vue';

interface Shelf {
    id: number;
    title: string;
}

const props = defineProps<{ shelves: Shelf[]; closed: Shelf | null }>();

const homeUrl = route('start');

const catalogue = computed(() => ({ rows: props.shelves }));

function visible() {
    return catalogue.value.rows.filter((row) => row.title.length > 0);
}
</script>

<template>
    <div class="shelf-wrap">
        <h1 class="spine-label">Shelves</h1>
        <Stamp v-for="s in visible()" :key="s.id" :label="s.title" />
    </div>
</template>
