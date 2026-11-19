<script setup lang="ts">
import { provide, ref } from 'vue';
import Cell from '../components/Cell.vue';
import ImportedCard from '../components/ImportedCard.vue';
import MessageActions from '../components/MessageActions.vue';
import Panel from '../components/Panel.vue';
import Wrapper from '../components/Wrapper.vue';
import { ThemeKey } from '../keys';
import { useCartStore } from '../stores/cart';
import { useChatStore } from '../stores/chat';

const chat = useChatStore();
const cart = useCartStore();
const bag = { title: 'a', tone: 'soft' };
const open = ref(false);
const text = ref('');

provide(ThemeKey, 'dark');
provide('locale', 'hu');

function onPurge(id: string): void {
    chat.send(id);
    useChatStore().clear();
    cart.add(id);
}
</script>

<template>
    <main>
        <MessageActions title="Hi" tone="soft" colr="red" @purge="onPurge" />
        <MessageActions tone="soft" @purge="onPurge" />
        <MessageActions
            :title="text"
            tone="loud"
            :count="1"
            id="a"
            class="b"
            data-x="1"
            aria-label="c"
            @click="onPurge('z')"
        >
            <template #footer>ok</template>
            <template #header>nope</template>
        </MessageActions>
        <message-actions v-bind="bag" />
        <Panel v-model="text" v-model:open="open" label="L">
            <template #head>h</template>
            <template v-slot:foot>f</template>
        </Panel>
        <Cell :cell-id="'1'" :cell-idx="2" />
        <Wrapper a="1" zzz="2" />
        <ImportedCard level="2" zzz="3" />
        <Fancy tone="x" />
    </main>
</template>
