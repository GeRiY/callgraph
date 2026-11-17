import { fileURLToPath, URL } from 'node:url';
import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vite';

export default defineConfig({
    plugins: [vue()],
    resolve: {
        alias: {
            '@kit': fileURLToPath(new URL('./src/kit', import.meta.url)),
        },
    },
    test: {
        environment: 'jsdom',
    },
});
