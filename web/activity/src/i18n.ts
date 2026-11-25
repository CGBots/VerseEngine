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
    // `message.attribut` : attribut Fluent (noms localisés des options de commandes slash).
    const [id, attr] = key.split('.', 2);
    const msg = bundle.getMessage(id);
    const pattern = attr ? msg?.attributes[attr] : msg?.value;
    if (!pattern) return key;
    return bundle.formatPattern(pattern, args);
  };
}
