import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api, ApiError, getSessionToken, travelApi, universeApi, type CharacterView, type MapView, type SessionInfo, type TravelState } from './api';
import { connect } from './discord';
import { loadTranslator, pickLocale, type Translator } from './i18n';
import { CharacterPanel } from './components/CharacterPanel';
import { MapView2D } from './components/MapView2D';
import { TravelPanel } from './components/TravelPanel';
import { TopBar } from './components/TopBar';
import { SettingsPanel } from './components/SettingsPanel';
import { MusicControl } from './components/MusicControl';
import { CloseIcon } from './components/icons/Icons';
import { roadPath, webglAvailable, type PlayerMotion } from './map/geometry';
import { playerPictureUrl } from './map/player';
import { connectRealtime } from './realtime';
import { useLayout } from './layout';

type State =
  | { kind: 'loading' }
  | { kind: 'error'; key: string }
  | { kind: 'ready'; session: SessionInfo };

const fallbackT: Translator = (k) => k;
// three.js est volumineux : chargé à part, après le panneau du personnage.
const MapScene = lazy(() => import('./components/MapScene').then((m) => ({ default: m.MapScene })));
// Import de carte (vue admin) : chargé seulement quand il est ouvert.
const ImportWizard = lazy(() => import('./admin/ImportWizard').then((m) => ({ default: m.ImportWizard })));
// Page admin de la musique : chargée seulement quand elle est ouverte.
const MusicAdmin = lazy(() => import('./admin/MusicAdmin').then((m) => ({ default: m.MusicAdmin })));
const errorKey = (e: unknown) => (e instanceof ApiError ? e.key : 'activity__internal_error');

/** Description du mouvement du joueur ; l'animation est faite par la carte (`useAnimatedPosition`). */
function usePlayerMotion(map: MapView | null, travel: TravelState | null): PlayerMotion | null {
  return useMemo(() => {
    if (!map || !travel?.location) return null;
    const loc = travel.location;
    if (loc.kind === 'place') {
      const p = map.places.find((x) => x.id === loc.id);
      return p ? { kind: 'place', at: { x: p.x, y: p.y } } : null;
    }
    const v = travel.travel;
    const from = map.places.find((x) => x.id === loc.source_id);
    const to = map.places.find((x) => x.id === loc.destination_id);
    if (!v || !from || !to) return null;
    return {
      kind: 'road',
      from: { x: from.x, y: from.y },
      to: { x: to.x, y: to.y },
      // Tracé réel de la route (import Azgaar), sinon ligne droite.
      path: roadPath(map, from.id, to.id),
      traveledKm: v.traveled_km,
      roadKm: v.traveled_km + (v.remaining_km ?? 0),
      speedKmh: v.speed_kmh,
      refTime: travel.server_time,
      moving: v.in_move,
    };
  }, [map, travel]);
}

/** `connectFn` : connexion à Discord (remplacée par une page de développement pour l'essayer hors de Discord). */
export function App({ connectFn = connect }: { connectFn?: typeof connect } = {}) {
  const [state, setState] = useState<State>({ kind: 'loading' });
  const [t, setT] = useState<Translator>(() => fallbackT);
  const [adminMode, setAdminMode] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  // Relecture de la playlist : déplacement, bibliothèque modifiée, reconnexion.
  const [musicVersion, setMusicVersion] = useState(0);
  const [musicAdminOpen, setMusicAdminOpen] = useState(false);
  const layout = useLayout();
  // Fiche du personnage : colonne (écran large, ouverte par défaut) ou volet (écran compact, fermé
  // par défaut). Le même bouton (avatar) l'ouvre ou la ferme partout.
  const [panelOpen, setPanelOpen] = useState(false);
  const [columnOpen, setColumnOpen] = useState(true);
  useEffect(() => setPanelOpen(false), [layout]);
  // Échap ferme le volet, où que soit le focus.
  useEffect(() => {
    if (!panelOpen) return;
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && setPanelOpen(false);
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [panelOpen]);
  const [character, setCharacter] = useState<CharacterView | null>(null);
  const [characterError, setCharacterError] = useState<string | null>(null);
  const [map, setMap] = useState<MapView | null>(null);
  const [travel, setTravel] = useState<TravelState | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  /** Incrémenté quand le bot rejoint ou quitte un serveur : l'écran d'import relit la liste. */
  const [serversVersion, setServersVersion] = useState(0);
  const [fogEnabled, setFogEnabled] = useState(false);
  const [fogBusy, setFogBusy] = useState(false);
  const use3d = useRef(webglAvailable()).current;

  const loadCharacter = useCallback(async () => {
    try {
      setCharacter(await api<CharacterView>('/me/character'));
      setCharacterError(null);
    } catch (e) {
      setCharacter(null);
      setCharacterError(errorKey(e));
    }
  }, []);
  const loadTravel = useCallback(async () => {
    try {
      setTravel(await travelApi.state());
    } catch {
      setTravel(null);
    }
  }, []);
  const loadMap = useCallback(async () => {
    try {
      setMap(await api<MapView>('/map'));
    } catch {
      setMap(null);
    }
  }, []);
  const reloadAll = useCallback(async () => {
    await Promise.all([loadCharacter(), loadTravel(), loadMap()]);
  }, [loadCharacter, loadTravel, loadMap]);

  const start = useCallback(async () => {
    setState({ kind: 'loading' });
    try {
      const { session, locale } = await connectFn();
      setT(await loadTranslator(pickLocale(locale)).then((tr) => () => tr));
      setState({ kind: 'ready', session });
      setFogEnabled(session.fog_enabled);
      // Sans univers, il n'y a encore ni personnage ni carte à charger.
      if (session.universe) await reloadAll();
    } catch (e) {
      setState({ kind: 'error', key: e instanceof ApiError ? e.key : 'activity__unauthorized' });
    }
  }, [reloadAll, connectFn]);

  useEffect(() => {
    void start();
  }, [start]);

  // Temps réel : une action faite par commande slash met aussi l'activité à jour.
  const ready = state.kind === 'ready';
  useEffect(() => {
    if (!ready) return;
    return connectRealtime(getSessionToken, (event) => {
      if (event === 'travel_updated') {
        void Promise.all([loadTravel(), loadCharacter()]);
        setMusicVersion((v) => v + 1);
      } else if (event === 'character_updated') void loadCharacter();
      else if (event === 'map_updated') void Promise.all([loadMap(), loadTravel()]);
      else if (event === 'servers_updated') setServersVersion((v) => v + 1);
      else if (event === 'music_updated') setMusicVersion((v) => v + 1);
      else {
        void reloadAll();
        setMusicVersion((v) => v + 1);
      }
    });
  }, [ready, loadTravel, loadCharacter, loadMap, reloadAll]);

  const act = useCallback(
    async (action: () => Promise<TravelState>) => {
      setBusy(true);
      setMessage(null);
      try {
        const next = await action();
        setTravel(next);
        if (next.result) setMessage(t(next.result));
        void loadCharacter();
        setMusicVersion((v) => v + 1);
      } catch (e) {
        setMessage(t(errorKey(e)));
      } finally {
        setBusy(false);
      }
    },
    [t, loadCharacter],
  );

  const motion = usePlayerMotion(map, travel);
  const reachable = useMemo(() => new Set(travel?.destinations.map((d) => d.id) ?? []), [travel]);
  const currentPlaceId = travel?.location?.kind === 'place' ? travel.location.id : null;

  if (state.kind === 'loading') return <div className="center">{t('activity_ui__loading')}</div>;
  if (state.kind === 'error')
    return (
      <div className="center">
        <p>{t(state.key)}</p>
        <button type="button" onClick={() => void start()}>{t('activity_ui__retry')}</button>
      </div>
    );

  const { session } = state;
  // Fin d'un import : nouvelle connexion silencieuse, la session est alors reliée à l'univers.
  const afterImport = () => {
    setImportOpen(false);
    void start();
  };
  if (!session.universe)
    return session.can_import ? (
      <div className="import-page">
        <p className="muted">{t('activity_ui__no_universe_admin')}</p>
        <Suspense fallback={<div className="center">{t('activity_ui__loading')}</div>}>
          <ImportWizard t={t} universeName={null} serversVersion={serversVersion} onDone={afterImport} />
        </Suspense>
      </div>
    ) : (
      <div className="center">
        <p>{t('activity_ui__no_universe_yet')}</p>
      </div>
    );
  const universe = session.universe;
  const avatar = session.user.avatar ? `https://cdn.discordapp.com/avatars/${session.user.id}/${session.user.avatar}.png?size=128` : null;
  const mapProps = {
    currentPlaceId,
    reachable,
    motion,
    playerPicture: playerPictureUrl(session.user, character),
    playerName: character?.name ?? session.user.global_name ?? session.user.username,
    onSelectPlace: setSelected,
  };
  const panel = character ? (
    <CharacterPanel character={character} avatarUrl={avatar} t={t} onOpenInventory={() => { /* phase c */ }} onResync={() => void loadCharacter()} />
  ) : (
    <aside className="side-panel"><p>{t(characterError ?? 'activity__no_character')}</p></aside>
  );
  const open = layout === 'wide' ? columnOpen : panelOpen;
  const togglePanel = () => (layout === 'wide' ? setColumnOpen((o) => !o) : setPanelOpen((o) => !o));
  return (
    <div className={`layout ${layout}${layout === 'wide' && !columnOpen ? ' column-closed' : ''}`}>
      {layout === 'wide' ? (
        columnOpen && <div id="character-panel" className="side-column">{panel}</div>
      ) : (
        <>
          {/* Écran compact : la même fiche, en volet par-dessus la carte. */}
          {panelOpen && <div className="drawer-backdrop" onClick={() => setPanelOpen(false)} />}
          <div id="character-panel" className={`drawer${panelOpen ? ' open' : ''}`} aria-hidden={!panelOpen}>
            <button type="button" className="icon-button drawer-close" aria-label={t('activity_ui__close')} onClick={() => setPanelOpen(false)}>
              <CloseIcon />
            </button>
            {panel}
          </div>
        </>
      )}
      <main className="map-area">
        <TopBar
          characterButton={{ picture: mapProps.playerPicture ?? character?.image_url ?? null, open, onToggle: togglePanel }}
          universeName={universe.name}
          clockParams={session.universe_clock}
          adminAvailable={session.admin_mode_available}
          adminMode={adminMode}
          onToggleAdmin={() => setAdminMode((on) => !on)}
          onOpenSettings={() => setSettingsOpen(true)}
          t={t}
        />
        <div className="map-view">
          {map ? (
            // Export Azgaar : son image, en 2D. Sinon : scène 3D, ou 2D sans WebGL.
            map.background || !use3d ? (
              <MapView2D map={map} {...mapProps} />
            ) : (
              <Suspense fallback={<MapView2D map={map} {...mapProps} />}>
                <MapScene map={map} {...mapProps} />
              </Suspense>
            )
          ) : (
            <div className="map-placeholder">{t('activity_ui__map')}</div>
          )}
          {travel && (
            <TravelPanel
              state={travel}
              selectedId={selected}
              busy={busy}
              message={message}
              speed={character?.speed ?? null}
              compact={layout === 'compact'}
              t={t}
              onStart={(id) => void act(() => travelApi.start(id))}
              onStop={() => void act(travelApi.stop)}
              onLeave={() => void act(travelApi.leave)}
              onElapsed={() => {
                void Promise.all([loadTravel(), loadCharacter()]);
                setMusicVersion((v) => v + 1);
              }}
            />
          )}
          <MusicControl t={t} version={musicVersion} />
        </div>
      </main>
      {adminMode && !importOpen && !musicAdminOpen && (session.can_import || session.access === 'admin') && (
        <div className="admin-actions">
          {session.can_import && (
            <button type="button" className="import-open" onClick={() => setImportOpen(true)}>{t('activity_ui__import_title')}</button>
          )}
          {session.access === 'admin' && (
            <button type="button" className="import-open" onClick={() => setMusicAdminOpen(true)}>{t('activity_ui__music_admin_title')}</button>
          )}
        </div>
      )}
      {musicAdminOpen && (
        <div className="dialog-backdrop">
          <div className="dialog music-dialog">
            <Suspense fallback={<p>{t('activity_ui__working')}</p>}>
              <MusicAdmin t={t} onClose={() => setMusicAdminOpen(false)} />
            </Suspense>
          </div>
        </div>
      )}
      {importOpen && (
        <div className="dialog-backdrop">
          <div className="dialog import-dialog">
            <Suspense fallback={<p>{t('activity_ui__loading')}</p>}>
              <ImportWizard t={t} universeName={universe.name} serversVersion={serversVersion} onClose={() => setImportOpen(false)} onDone={afterImport} />
            </Suspense>
          </div>
        </div>
      )}
      {settingsOpen && (
        <SettingsPanel
          t={t}
          onClose={() => setSettingsOpen(false)}
          fog={session.is_creator ? {
            enabled: fogEnabled,
            busy: fogBusy,
            onToggle: (enabled) => {
              setFogBusy(true);
              universeApi.setFog(enabled)
                .then((r) => { setFogEnabled(r.fog_enabled); return loadMap(); })
                .catch((e) => setMessage(t(errorKey(e))))
                .finally(() => setFogBusy(false));
            },
          } : undefined}
        />
      )}
    </div>
  );
}
