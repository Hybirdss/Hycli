import { createContext, useContext, useMemo, type ReactNode } from 'react';
import english from './locales/en.json';

export const languages = [
  ['en', 'English'], ['ko', '한국어'], ['ja', '日本語'], ['zh-CN', '简体中文'], ['zh-TW', '繁體中文'],
  ['es', 'Español'], ['fr', 'Français'], ['de', 'Deutsch'], ['pt-BR', 'Português (Brasil)'], ['id', 'Bahasa Indonesia'],
  ['it', 'Italiano'], ['nl', 'Nederlands'], ['pl', 'Polski'], ['tr', 'Türkçe'], ['ru', 'Русский'],
  ['uk', 'Українська'], ['vi', 'Tiếng Việt'], ['th', 'ไทย'], ['ar', 'العربية'], ['hi', 'हिन्दी'],
] as const;
const modules = import.meta.glob<{ default: Record<string, string> }>('./locales/*.json', { eager: true });
const fallback = english as Record<string, string>;
export type Translate = (key: string, vars?: Record<string, string | number>) => string;
type LocaleContext = { locale: string; t: Translate; date: (value: string, short?: boolean) => string; number: (value: number) => string };
const Context = createContext<LocaleContext>({ locale: 'en', t: key => fallback[key] || '', date: value => value, number: String });
export function I18n({ locale, children }: { locale: string; children: ReactNode }) {
  const value = useMemo<LocaleContext>(() => {
    const messages = modules[`./locales/${locale}.json`]?.default || fallback;
    const t: Translate = (key, vars = {}) => (messages[key] || fallback[key] || fallback['errors.internal']).replace(/\{(\w+)\}/g, (match, field: string) => String(vars[field] ?? match));
    return {
      locale, t,
      date: (date, short = false) => { const value = new Date(date); return Number.isNaN(value.getTime()) ? t('common.never') : new Intl.DateTimeFormat(locale, short ? { month: 'short', day: 'numeric' } : { dateStyle: 'medium', timeStyle: 'short' }).format(value); },
      number: value => new Intl.NumberFormat(locale).format(value),
    };
  }, [locale]);
  return <Context.Provider value={value}>{children}</Context.Provider>;
}
export const useI18n = () => useContext(Context);
export function initialLocale() {
  const stored = localStorage.getItem('hycli.locale');
  if (languages.some(([id]) => id === stored)) return stored!;
  return 'en';
}
