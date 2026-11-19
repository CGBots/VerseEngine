import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api, ApiError, clock, getSessionToken, travelApi, type CharacterView, type MapView, type SessionInfo, type TravelState } from './api';
import { connect } from './discord';
import { loadTranslator, pickLocale, type Translator } from './i18n';
import { CharacterPanel } from './components/CharacterPanel';
import { MapView2D } from './components/MapView2D';
import { TravelPanel } from './components/TravelPanel';
import { TopBar } from './components/TopBar';
import { SettingsPanel } from './components/SettingsPanel';
import { positionOnRoad, traveledAt, webglAvailable, type MapPoint } from './map/geometry';
import { playerPictureUrl } from './map/player';
import { connectRealtime } from './realtime';

type State =
  | { kind: 'loading' }
  | { kind: 'error'; key: string }
  | { kind: 'ready'; session: SessionInfo };

const fallbackT: Translator = (k) => k;
// three.js est volumineux : chargé à part, après le panneau du personnage.
const MapScene = lazy(() => import('./components/MapScene').then((m) => ({ default: m.MapScene })));
const errorKey = (e: unknown) => (e instanceof ApiError ? e.key : 'activity__internal_error');

/** Position du joueur dans la scène, recalculée chaque seconde pendant un voyage. */
function usePlayerPosition(map: MapView | null, travel: TravelState | null): MapPoint | null {
  const [tick, setTick] = useState(0);
  const moving = !!travel?.travel?.in_move;
  useEffect(() => {
    if (!moving) return;
    const id = window.setInterval(() => setTick((n) => n + 1), 1000);
    return () => window.clearInterval(id);
  }, [moving]);
  return useMemo(() => {
    void tick;
    if (!map || !travel?.location) return null;
    const loc = travel.location;
    if (loc.kind === 'place') {
      const p = map.places.find((x) => x.id === loc.id);
      return p ? { x: p.x, y: p.y } : null;
    }
    const v = travel.travel;
    if (!v || !loc.source_id || !loc.destination_id) return null;
    const roadKm = v.traveled_km + (v.remaining_km ?? 0);
    const traveled = v.in_move ? traveledAt(v.traveled_km, v.speed_kmh, travel.server_time, clock.now()) : v.traveled_km;
    return positionOnRoad(map, loc.source_id, loc.destination_id, traveled, roadKm);
  }, [map, travel, tick]);
}

export function App() {
  const [state, setState] = useState<State>({ kind: 'loading' });
  const [t, setT] = useState<Translator>(() => fallbackT);
  const [adminMode, setAdminMode] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [character, setCharacter] = useState<CharacterView | null>(null);
  const [characterError, setCharacterError] = useState<string | null>(null);
  const [map, setMap] = useState<MapView | null>(null);
  const [travel, setTravel] = useState<TravelState | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
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
      const { session, locale } = await connect();
      setT(await loadTranslator(pickLocale(locale)).then((tr) => () => tr));
      setState({ kind: 'ready', session });
      await reloadAll();
    } catch (e) {
      setState({ kind: 'error', key: e instanceof ApiError ? e.key : 'activity__unauthorized' });
    }
  }, [reloadAll]);

  useEffect(() => {
    void start();
  }, [start]);

  // Temps réel : une action faite par commande slash met aussi l'activité à jour.
  const ready = state.kind === 'ready';
  useEffect(() => {
    if (!ready) return;
    return connectRealtime(getSessionToken, (event) => {
      if (event === 'travel_updated') void Promise.all([loadTravel(), loadCharacter()]);
      else if (event === 'character_updated') void loadCharacter();
      else if (event === 'map_updated') void Promise.all([loadMap(), loadTravel()]);
      else void reloadAll();
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
      } catch (e) {
        setMessage(t(errorKey(e)));
      } finally {
        setBusy(false);
      }
    },
    [t, loadCharacter],
  );

  const player = usePlayerPosition(map, travel);
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
  const avatar = session.user.avatar ? `https://cdn.discordapp.com/avatars/${session.user.id}/${session.user.avatar}.png?size=128` : null;
  const mapProps = {
    currentPlaceId,
    reachable,
    player,
    playerPicture: playerPictureUrl(session.user, character),
    playerName: character?.name ?? session.user.global_name ?? session.user.username,
    onSelectPlace: setSelected,
  };
  return (
    <div className="layout">
      {character ? (
        <CharacterPanel character={character} avatarUrl={avatar} t={t} onOpenInventory={() => { /* phase c */ }} onResync={() => void loadCharacter()} />
      ) : (
        <aside className="side-panel"><p>{t(characterError ?? 'activity__no_character')}</p></aside>
      )}
      <main className="map-area">
        <TopBar
          universeName={session.universe.name}
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
              t={t}
              onStart={(id) => void act(() => travelApi.start(id))}
              onStop={() => void act(travelApi.stop)}
              onLeave={() => void act(travelApi.leave)}
              onElapsed={() => void Promise.all([loadTravel(), loadCharacter()])}
            />
          )}
        </div>
      </main>
      {settingsOpen && <SettingsPanel t={t} onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
