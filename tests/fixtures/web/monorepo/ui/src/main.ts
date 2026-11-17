import { createApp } from 'vue';
import { router } from './router';
import OverviewView from './views/OverviewView.vue';

createApp(OverviewView).use(router).mount('#app');
