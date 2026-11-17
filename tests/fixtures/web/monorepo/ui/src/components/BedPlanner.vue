<script setup lang="ts">
import { computed, ref } from 'vue';
import type { BedSummary } from '@common/calendar';
import { t } from '../lib/messages';
import CellMenu from './CellMenu.vue';

interface BedPlannerProps {
    bed: BedSummary;
    locked?: boolean;
    cells: string[];
    note: string;
    wateredNames: string[];
    focusCellId?: string;
    pinnedCellId?: string;
    canSow: boolean;
    canRemove: boolean;
    canMove: boolean;
    canLabel: boolean;
    canPhoto: boolean;
    plantCount: number;
    bedCount: number;
    sensorCount: number;
    linkState: 'open' | 'pending' | 'closed';
    placeholder?: string;
    saveLabel?: string;
    maxCells?: number;
    dense?: boolean;
    showLabels?: boolean;
    showDates?: boolean;
    locale?: string;
}

const props = defineProps<BedPlannerProps>();

const emit = defineEmits<{
    (e: 'sowSeed', id: string): void;
    (e: 'addBed', id: string): void;
    (e: 'archiveBed'): void;
    (e: 'renameBed', label: string): void;
    (e: 'clearNote'): void;
    (e: 'closePlanner'): void;
    (e: 'shareLayout', url: string): void;
    (e: 'skipWatering', id: string): void;
    (e: 'moveCell', id: string, to: number): void;
    (e: 'labelCell', id: string, text: string): void;
    (e: 'jumpToCell', id: string): void;
    (e: 'leaveBed'): void;
    (e: 'showArchive'): void;
    (e: 'markWatered', id: string): void;
    (e: 'snooze', hours: number): void;
    (e: 'pinCell', id: string): void;
    (e: 'remove', id: string): void;
    (e: 'tag', id: string, tag: string): void;
    (e: 'copyPlan', id: string): void;
    (e: 'retryStore', id: string): void;
    (e: 'save', text: string): void;
    (e: 'startNote'): void;
    (e: 'stopNote'): void;
    (e: 'unpinCell'): void;
    (e: 'updateNote', text: string): void;
}>();

const grid = ref<HTMLElement | null>(null);
const chosenId = ref<string | null>(null);
const menuOpen = ref(false);

const statusLine = computed(() => (props.locked ? t('bed.locked') : props.placeholder ?? ''));
const sowingLine = computed(() => props.wateredNames.map((plant) => t('bed.sowing', { plant })).join(', '));
const storable = computed(() => !props.locked && props.note.trim().length > 0);

function store(): void {
    if (!storable.value) {
        return;
    }
    emit('save', props.note);
    emit('clearNote');
}

function onNoteInput(event: Event): void {
    const value = (event.target as HTMLTextAreaElement).value;
    emit('updateNote', value);
    emit(value.length > 0 ? 'startNote' : 'stopNote');
}

function choose(id: string): void {
    chosenId.value = chosenId.value === id ? null : id;
    menuOpen.value = chosenId.value !== null;
}

function scrollToEnd(): void {
    const el = grid.value;
    if (el) {
        el.scrollTop = el.scrollHeight;
    }
}

function dropCell(id: string): void {
    emit('remove', id);
    chosenId.value = null;
}

defineExpose({ scrollToEnd });
</script>

<template>
    <section class="bed-planner" :class="{ dense }">
        <header>
            <h2>{{ bed.label }}</h2>
            <button type="button" @click="emit('closePlanner')">x</button>
            <button v-if="canSow" type="button" @click="emit('sowSeed', '')">+</button>
        </header>
        <ul ref="grid">
            <li v-for="(cell, index) in cells" :key="index" @click="choose(String(index))">
                {{ cell }}
                <CellMenu
                    v-if="chosenId === String(index)"
                    :cell-id="String(index)"
                    :can-remove="canRemove"
                    @remove="dropCell"
                />
            </li>
        </ul>
        <p>{{ sowingLine }}</p>
        <p>{{ statusLine }}</p>
        <textarea :value="note" :disabled="locked" @input="onNoteInput" />
        <button type="button" :disabled="!storable" @click="store">{{ saveLabel ?? t('bed.save') }}</button>
    </section>
</template>
