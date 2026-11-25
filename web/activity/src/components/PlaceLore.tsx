import { useEffect, useState } from 'react';
import { placeApi, type BurgInfo, type PlaceLore as Lore } from '../api';
import type { Translator } from '../i18n';
import { formatPopulation } from '../placeSheet';
import { CloseIcon } from './icons/Icons';

const SIZE_KEY = ['place_wiki__size_village', 'place_wiki__size_town', 'place_wiki__size_city'];
const FEATURE_KEY: Record<string, string> = {
  citadel: 'place_wiki__feature_citadel',
  walls: 'place_wiki__feature_walls',
  plaza: 'place_wiki__feature_plaza',
  temple: 'place_wiki__feature_temple',
  shanty: 'place_wiki__feature_shanty',
};

/** Résumé du bourg (taille, capitale, port, bâtiments), commun aux fiches joueur et admin. */
export function burgSummary(burg: BurgInfo, t: Translator): string {
  return [t(SIZE_KEY[Math.min(burg.size, 2)]), burg.capital && t('place_wiki__capital'), burg.port && t('place_wiki__port'), ...burg.features.map((f) => FEATURE_KEY[f] && t(FEATURE_KEY[f]))]
    .filter(Boolean)
    .join(', ');
}

/** Fiche d'un lieu pour un joueur qui le clique sur la carte, en lecture seule : informations du
 *  bourg, routes connues et texte publié. Rien ne s'affiche si le serveur refuse (lieu non
 *  découvert sous le brouillard) : la fenêtre de voyage reste la seule réponse au clic. */
export function PlaceLore({ placeId, t, locale, onClose }: { placeId: string; t: Translator; locale: string; onClose: () => void }) {
  const [lore, setLore] = useState<Lore | null>(null);
  useEffect(() => {
    let alive = true;
    setLore(null);
    placeApi
      .lore(placeId)
      .then((l) => alive && setLore(l))
      .catch(() => alive && setLore(null));
    return () => {
      alive = false;
    };
  }, [placeId]);
  if (!lore) return null;
  const burg = lore.burg;
  return (
    <aside className="place-lore" aria-label={t('activity_ui__place_lore')}>
      <header className="window-header">
        <h2>{lore.name}</h2>
        <button type="button" className="icon-button" aria-label={t('activity_ui__close')} title={t('activity_ui__close')} onClick={onClose}>
          <CloseIcon size={18} />
        </button>
      </header>
      <dl className="place-facts">
        {burg && (
          <>
            <dt>{t('activity_ui__place_burg')}</dt>
            <dd>{burgSummary(burg, t)}</dd>
            {burg.population != null && (
              <>
                <dt>{t('place_wiki__population')}</dt>
                <dd>{formatPopulation(burg.population, locale)}</dd>
              </>
            )}
            {burg.state && (
              <>
                <dt>{t('place_wiki__state')}</dt>
                <dd>{burg.state}</dd>
              </>
            )}
            {burg.culture && (
              <>
                <dt>{t('place_wiki__culture')}</dt>
                <dd>{burg.culture}</dd>
              </>
            )}
          </>
        )}
        <dt>{t('activity_ui__place_roads')}</dt>
        <dd>
          {lore.roads.length === 0
            ? t('activity_ui__place_no_road')
            : lore.roads.map((r) => `${r.to_name} (${r.distance_km} km)`).join(' · ')}
        </dd>
      </dl>
      {lore.text && <p className="lore-text">{lore.text}</p>}
    </aside>
  );
}
