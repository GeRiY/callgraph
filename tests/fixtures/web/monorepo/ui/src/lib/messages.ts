import { createI18n } from 'vue-i18n';
import { en } from '../locales/en';
import { de } from '../locales/de';

export const i18n = createI18n({
    legacy: false,
    locale: 'en',
    messages: { en, de },
});

export function t(key: string, params?: Record<string, unknown>): string {
    return i18n.global.t(key, params ?? {});
}
