import { defineStore } from 'pinia';
import { ref } from 'vue';

export const useCartStore = defineStore('cart', () => {
    const lines = ref<string[]>([]);

    function add(line: string) {
        lines.value.push(line);
    }

    function reset() {
        lines.value = [];
    }

    return { lines, add, reset };
});
