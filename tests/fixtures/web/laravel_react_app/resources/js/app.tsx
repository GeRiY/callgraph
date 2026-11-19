import { createRoot } from 'react-dom/client';
import { createInertiaApp } from '@inertiajs/react';

const pages = import.meta.glob('./Pages/**/*.tsx');

createInertiaApp({
    resolve: (name) => pages[`./Pages/${name}.tsx`](),
    setup({ el, App, props }) {
        createRoot(el).render(<App {...props} />);
    },
});
