import { FluentBundle, FluentResource, type FluentVariable } from '@fluent/bundle';

/** Les traductions viennent du bot (`translations/*.ftl`) via `/api/i18n/{locale}` : source unique. */
export type Translator = (key: string, args?: Record<string, FluentVariable>) => string;

export function pickLocale(discordLocale: string | undefined): 'fr' | 'en-US' {
  return discordLocale?.toLowerCase().startsWith('fr') ? 'fr' : discordLocale ? 'en-US' : 'fr';
}

export async function loadTranslator(locale: 'fr' | 'en-US'): Promise<Translator> {
  const bundle = new FluentBundle(locale, { useIsolating: false });
  try {
    const res = await fetch(`/api/i18n/${locale}`);
    if (res.ok) bundle.addResource(new FluentResource(await res.text()));
  } catch {
    /* les clés brutes seront affichées */
  }
  return (key, args) => {
    const msg = bundle.getMessage(key);
    if (!msg?.value) return key;
    return bundle.formatPattern(msg.value, args);
  };
}
