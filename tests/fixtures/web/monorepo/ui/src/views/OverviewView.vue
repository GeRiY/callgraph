<script setup lang="ts">
import { ref } from 'vue';
import Tag from '@kit/Tag.vue';
import BedPlanner from '../components/BedPlanner.vue';
import { remindSeason } from '../lib/useReminders';
import { t } from '../lib/messages';

const cells = ref<string[]>(['tomato']);

function dropCell(id: string): void {
    cells.value.splice(Number(id), 1);
}
</script>

<template>
    <main>
        <h1>{{ t('overview.title') }}</h1>
        <Tag :text="remindSeason('spring')" />
        <BedPlanner
            :bed="{ id: '1', label: 'North' }"
            :cells="cells"
            note=""
            :watered-names="[]"
            :can-sow="true"
            :can-remove="true"
            :can-move="false"
            :can-label="false"
            :can-photo="false"
            :plant-count="0"
            :bed-count="1"
            :sensor-count="1"
            link-state="open"
            @remove="dropCell"
        />
    </main>
</template>
