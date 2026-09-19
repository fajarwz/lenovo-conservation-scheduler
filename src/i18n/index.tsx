// Translation for the window. The Rust side has its own table (src-tauri/src/i18n.rs) because the
// tray menu and Windows notifications never pass through here; both read the same `locale` from
// the config file, so the two stay in step.
import { createContext, useContext, useMemo, type ReactNode } from "react";
import { enUS } from "./en-US";
import { id } from "./id";

export type TranslationKey = keyof typeof enUS;
export type TranslationParams = Record<string, string | number>;
export type Locale = "en-US" | "id";

/** Offered in the language dropdown: each language's own name, never translated. */
export const LOCALES: { value: Locale; label: string }[] = [
  { value: "en-US", label: "English" },
  { value: "id", label: "Bahasa Indonesia" },
];

const DICTIONARIES: Record<Locale, typeof enUS> = { "en-US": enUS, id };

export function translate(
  locale: Locale,
  key: TranslationKey,
  params?: TranslationParams,
): string {
  // Falling back to English beats rendering an empty label if a key ever goes missing.
  const template = DICTIONARIES[locale]?.[key] ?? enUS[key];
  if (!params) {
    return template;
  }
  return template.replace(/\{(\w+)\}/g, (placeholder, name: string) =>
    name in params ? String(params[name]) : placeholder,
  );
}

const LocaleContext = createContext<Locale>("en-US");

export function LocaleProvider({ locale, children }: { locale: Locale; children: ReactNode }) {
  return <LocaleContext.Provider value={locale}>{children}</LocaleContext.Provider>;
}

/**
 * `const { t } = useTranslation()` in a component: re-renders when the language changes.
 * The locale lives in the saved config, so switching language saves immediately.
 */
export function useTranslation() {
  const locale = useContext(LocaleContext);
  return useMemo(
    () => ({
      locale,
      t: (key: TranslationKey, params?: TranslationParams) => translate(locale, key, params),
    }),
    [locale],
  );
}
