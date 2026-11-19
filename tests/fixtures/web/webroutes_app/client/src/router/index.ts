import { createRouter, createWebHistory, type RouteRecordRaw } from 'vue-router';
import HomePage from '../pages/HomePage.vue';

const routes: RouteRecordRaw[] = [
    { path: '/', name: 'home', component: HomePage },
    {
        path: '/chats',
        component: () => import('../layouts/ChatLayout.vue'),
        children: [
            { path: '', name: 'chat-list', component: () => import('../pages/ChatListPage.vue') },
            { path: ':id', name: 'chat', component: () => import('../pages/ChatPage.vue') },
        ],
    },
    // old addresses
    { path: '/old-home', redirect: '/' },
    { path: '/start', redirect: { name: 'home' } },
];

export const router = createRouter({ history: createWebHistory(), routes });
