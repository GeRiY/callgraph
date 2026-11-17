import { createRouter, createWebHistory } from 'vue-router';

export const router = createRouter({
    history: createWebHistory(),
    routes: [
        { path: '/', name: 'overview', component: () => import('../views/OverviewView.vue') },
        { path: '/beds/:id', name: 'bed', component: () => import('../views/BedView.vue') },
    ],
});
