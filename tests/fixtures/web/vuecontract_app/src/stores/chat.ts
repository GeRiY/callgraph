import { defineStore } from 'pinia';

export const useChatStore = defineStore('chat', {
    state: () => ({ items: [] as string[], draft: '' }),
    getters: {
        count: (state) => state.items.length,
    },
    actions: {
        send(text: string) {
            this.items.push(text);
        },
        clear() {
            this.items = [];
        },
    },
});
