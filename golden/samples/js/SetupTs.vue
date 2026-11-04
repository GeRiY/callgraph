<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue';
import { router } from '@inertiajs/vue3';

interface Props {
  title: string;
  count?: number;
}
const props = defineProps<Props>();
const emit = defineEmits<{ (e: 'close'): void; (e: 'update', v: number): void }>();

const current = ref<number>(props.count ?? 0);
const label = computed(() => `${props.title}: ${current.value}`);

function increment(step: number): void {
  current.value += step;
  emit('update', current.value);
}

watch(current, (v) => {
  if (v > 5) {
    emit('close');
  } else {
    router.reload();
  }
});

onMounted(() => {
  increment(1);
});
</script>

<template>
  <section>
    <h2>{{ label }}</h2>
    <button @click="increment(2)">Add</button>
    <a :href="route('sample.home')">Home</a>
  </section>
</template>
