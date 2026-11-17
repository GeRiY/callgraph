<script setup lang="ts">
import { t } from '../lib/messages';

interface Props {
    cellId: string;
    canRemove: boolean;
    canMove?: boolean;
}

const props = defineProps<Props>();

const emit = defineEmits<{
    (e: 'remove', id: string): void;
    (e: 'move', id: string): void;
    (e: 'duplicate', id: string): void;
}>();

function choose(action: 'remove' | 'move' | 'duplicate'): void {
    if (action === 'remove') {
        emit('remove', props.cellId);
    } else if (action === 'move') {
        emit('move', props.cellId);
    } else {
        emit('duplicate', props.cellId);
    }
}
</script>

<template>
    <div class="cell-menu" :aria-label="t('cell.menu')">
        <button v-if="canRemove" type="button" @click="choose('remove')">{{ t('cell.remove') }}</button>
        <button v-if="canMove" type="button" @click="choose('move')">move</button>
        <button type="button" @click="choose('duplicate')">copy</button>
    </div>
</template>
