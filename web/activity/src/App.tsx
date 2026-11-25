import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  api, ApiError, craftApi, newcomerApi, farmApi, getSessionToken, mapPath, refreshSession, travelApi, universeApi,
  type CharacterView, type CraftState, type FarmState, type InventoryView, type MapView, type SpectatorGroup, type RecipeBook, type SessionInfo, type TravelState,
} from './api';
import { connect } from './discord';
import { loadTranslator, pickLocale, type Translator } from './i18n';
import { CharacterPanel } from './components/CharacterPanel';
import { MapView2D } from './components/MapView2D';
import { TravelPanel } from './components/TravelPanel';
import { ActionBar, AdminActionBar, type AdminWindow } from './components/ActionBar';
import { CraftWindow } from './components/CraftWindow';
import { InventoryWindow } from './components/InventoryWindow';
import { availability, currentOccupation } from './occupation';
import { TopBar } from './components/TopBar';
import { SettingsPanel } from './components/SettingsPanel';
import { MusicControl } from './components/MusicControl';
import { CloseIcon } from './components/icons/Icons';
import { webglAvailable, type PlayerMotion } from './map/geometry';
import { motionOf, type OtherGroup } from './map/motion';
import { Newcomer } from './components/Newcomer';
import { LootToast, type LootToastData } from './components/LootToast';
import { playerPictureUrl } from './map/player';
import { coalesce, connectRealtime, type ServerEvent } from './realtime';
import { useLayout } from './layout';
import { UniverseGenesis } from './components/UniverseGenesis';
import { PlaceSheet } from './components/PlaceSheet';
import { PlaceLore } from './components/PlaceLore';

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
// Fenêtres du mode admin (barre d'actions admin) : chargées à leur première ouverture.
const TeleportWindow = lazy(() => import('./admin/TeleportWindow').then((m) => ({ default: m.TeleportWindow })));
const ItemEditor = lazy(() => import('./admin/ItemEditor').then((m) => ({ default: m.ItemEditor })));
const RecipeEditor = lazy(() => import('./admin/RecipeEditor').then((m) => ({ default: m.RecipeEditor })));
const errorKey = (e: unknown) => (e instanceof ApiError ? e.key : 'activity__internal_error');

/** Description du mouvement du joueur ; l'animation est faite par la carte (`useAnimatedPosition`). */
function usePlayerMotion(map: MapView | null, travel: TravelState | null): PlayerMotion | null {
  return useMemo(() => (map && travel ? motionOf(map, travel.location, travel.travel, travel.server_time) : null), [map, travel]);
}

/** Spectateur : relecture des groupes, l'évènement temps réel des voyages ne visant que leurs membres. */
const SPECTATOR_REFRESH_MS = 15_000;

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
  // Sans personnage : regarder en spectateur (carte dévoilée, tous les groupes).
  const [spectatorChoice, setSpectatorChoice] = useState(false);
  const [spectatorGroups, setSpectatorGroups] = useState<SpectatorGroup[] | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  /** Incrémenté quand le bot rejoint ou quitte un serveur : l'écran d'import relit la liste. */
  const [serversVersion, setServersVersion] = useState(0);
  const [fogEnabled, setFogEnabled] = useState(false);
  const [fogBusy, setFogBusy] = useState(false);
  /** Craft et farm en cours, relus depuis le serveur (`/me/craft`, `/me/farm`). */
  const [craftState, setCraftState] = useState<CraftState | null>(null);
  const [farmState, setFarmState] = useState<FarmState | null>(null);
  // Notification du butin d'une fouille terminée (quelques secondes).
  const [lootToast, setLootToast] = useState<LootToastData | null>(null);
  const closeLootToast = useCallback(() => setLootToast(null), []);
  const shownFarm = useRef<number | null>(null);
  useEffect(() => {
    const f = farmState?.finished;
    if (!f) return;
    // Au premier chargement, une fouille finie avant l'ouverture n'est pas annoncée.
    const first = shownFarm.current === null;
    if (f.at === shownFarm.current) return;
    shownFarm.current = f.at;
    if (first && farmState && farmState.server_time - f.at > 30) return;
    setLootToast({ id: `farm-${f.at}`, items: f.items.filter((i) => !i.unknown) });
  }, [farmState]);
  /** Inventaire et livre de recettes : chargés à l'ouverture de la fenêtre de craft. */
  const [inventory, setInventory] = useState<InventoryView | null>(null);
  const [book, setBook] = useState<RecipeBook | null>(null);
  /** Fenêtres demandées par le joueur (l'occupation en cours peut les forcer ou les fermer). */
  const [travelOpen, setTravelOpen] = useState(false);
  const [craftOpen, setCraftOpen] = useState(false);
  const craftOpenRef = useRef(false);
  craftOpenRef.current = craftOpen;
  /** Inventaire du joueur : consultable à tout moment (simple lecture). */
  const [inventoryOpen, setInventoryOpen] = useState(false);
  const inventoryOpenRef = useRef(false);
  inventoryOpenRef.current = inventoryOpen;
  /** Fenêtre du mode admin ouverte (recettes, objets, téléportation). */
  const [adminWindow, setAdminWindow] = useState<AdminWindow | null>(null);
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const [craftMessage, setCraftMessage] = useState<string | null>(null);
  /** Animation de naissance de l'univers, jouée à la fin d'un import. */
  const [genesis, setGenesis] = useState<{ name: string } | null>(null);
  const use3d = useRef(webglAvailable()).current;
  /** Lieu cliqué : fiche du lieu en mode admin, texte publié sinon (`PlaceSheet`, `PlaceLore`). */
  const [placeSheetId, setPlaceSheetId] = useState<string | null>(null);
  const [loreId, setLoreId] = useState<string | null>(null);
  /** Incrémenté à chaque `map_updated` : la fiche ouverte se relit. */
  const [placeVersion, setPlaceVersion] = useState(0);
  const [uiLocale, setUiLocale] = useState<'fr' | 'en-US'>('fr');
  useEffect(() => {
    setPlaceSheetId(null);
    setLoreId(null);
  }, [adminMode]);

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
  // Mode admin : toute la carte (le serveur le vérifie) ; sinon, la carte d'un joueur, brouillard compris.
  const adminModeRef = useRef(adminMode);
  adminModeRef.current = adminMode;
  const spectating = spectatorChoice && !character;
  const spectatingRef = useRef(spectating);
  spectatingRef.current = spectating;
  const loadMap = useCallback(async () => {
    try {
      setMap(await api<MapView>(mapPath(adminModeRef.current, spectatingRef.current)));
    } catch {
      setMap(null);
    }
  }, []);
  const loadCraft = useCallback(async () => {
    try {
      setCraftState(await craftApi.state());
    } catch {
      setCraftState(null);
    }
  }, []);
  const loadFarm = useCallback(async () => {
    try {
      setFarmState(await farmApi.state());
    } catch {
      setFarmState(null);
    }
  }, []);
  /** Inventaire et livre de recettes, seulement pour les fenêtres ouvertes (sinon rien à
   *  afficher). `force` : la fenêtre qui s'ouvre (son état n'est pas encore rendu). */
  const loadCraftBook = useCallback(async (force: 'craft' | 'inventory' | null = null) => {
    const wantBook = force === 'craft' || craftOpenRef.current;
    const wantInventory = force === 'inventory' || inventoryOpenRef.current;
    if (!wantBook && !wantInventory) return;
    const [inv, rec] = await Promise.allSettled([
      wantInventory ? craftApi.inventory() : Promise.resolve(null),
      wantBook ? craftApi.recipes() : Promise.resolve(null),
    ]);
    if (wantInventory) setInventory(inv.status === 'fulfilled' ? inv.value : null);
    if (wantBook) setBook(rec.status === 'fulfilled' ? rec.value : null);
  }, []);
  const firstAdminRender = useRef(true);
  useEffect(() => {
    if (firstAdminRender.current) return void (firstAdminRender.current = false);
    void loadMap();
  }, [adminMode, spectating, loadMap]);
  // Spectateur : position de tous les groupes, relue régulièrement.
  useEffect(() => {
    if (!spectating) return setSpectatorGroups(null);
    let alive = true;
    const load = () => newcomerApi.players().then((r) => alive && setSpectatorGroups(r.groups)).catch(() => undefined);
    void load();
    const id = window.setInterval(load, SPECTATOR_REFRESH_MS);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, [spectating]);
  const reloadAll = useCallback(async () => {
    await Promise.all([loadCharacter(), loadTravel(), loadMap(), loadCraft(), loadFarm(), loadCraftBook()]);
  }, [loadCharacter, loadTravel, loadMap, loadCraft, loadFarm, loadCraftBook]);

  const start = useCallback(async () => {
    setState({ kind: 'loading' });
    try {
      const { session, locale } = await connectFn();
      const uiLoc = pickLocale(locale);
      setUiLocale(uiLoc);
      setT(await loadTranslator(uiLoc).then((tr) => () => tr));
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
    // Une rafale d'évènements (import de carte) ne relit chaque ressource qu'une fois.
    const batch = coalesce((events) => {
      if (events.includes('resync')) {
        setMusicVersion((v) => v + 1);
        return void reloadAll();
      }
      const has = (e: ServerEvent) => events.includes(e);
      const jobs: Promise<void>[] = [];
      if (has('map_updated')) jobs.push(loadMap());
      if (has('map_updated')) setPlaceVersion((v) => v + 1);
      if (has('travel_updated') || has('map_updated')) jobs.push(loadTravel());
      if (has('travel_updated') || has('character_updated')) jobs.push(loadCharacter());
      if (has('craft_updated')) jobs.push(loadCraft());
      if (has('farm_updated')) jobs.push(loadFarm());
      // Les outils posés dépendent aussi de la position.
      if (has('inventory_updated') || has('craft_updated') || has('travel_updated')) jobs.push(loadCraftBook());
      if (has('servers_updated')) setServersVersion((v) => v + 1);
      // Playlist du lieu : relue après un déplacement ou un changement de la bibliothèque.
      if (has('travel_updated') || has('music_updated')) setMusicVersion((v) => v + 1);
      void Promise.all(jobs);
    });
    const stop = connectRealtime(getSessionToken, batch.push);
    return () => {
      batch.cancel();
      stop();
    };
  }, [ready, loadTravel, loadCharacter, loadMap, loadCraft, loadFarm, loadCraftBook, reloadAll]);

  // Une invitation reçue ouvre la fenêtre de voyage, où elle s'accepte.
  const inviteCount = travel?.invites?.length ?? 0;
  const seenInvites = useRef(0);
  useEffect(() => {
    if (inviteCount > seenInvites.current) setTravelOpen(true);
    seenInvites.current = inviteCount;
  }, [inviteCount]);
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

  /** Action de craft ou de farm : l'état renvoyé par le serveur remplace l'état local. */
  const runAction = useCallback(
    async <T,>(action: () => Promise<T>, apply: (next: T) => string | null, show: (m: string | null) => void) => {
      setBusy(true);
      show(null);
      try {
        show(apply(await action()));
      } catch (e) {
        show(t(errorKey(e)));
      } finally {
        setBusy(false);
      }
    },
    [t],
  );
  const startCraft = (recipeId: string, quantity: number) =>
    void runAction(
      () => craftApi.start(recipeId, quantity),
      (next) => {
        setCraftState(next);
        void loadCraftBook();
        return next.result ? t(next.result) : null;
      },
      setCraftMessage,
    );
  const stopCraft = () =>
    void runAction(
      craftApi.stop,
      (next) => {
        setCraftState(next);
        void loadCraftBook();
        return t('activity_ui__craft_stopped_detail', { delivered: next.delivered ?? 0, refunded: next.refunded ?? 0 });
      },
      setActionMessage,
    );
  const startFarm = () =>
    void runAction(
      farmApi.start,
      (next) => {
        setFarmState(next);
        void loadCraftBook();
        if (next.cooldown_secs != null) return t('activity_ui__farm_cooldown', { seconds: next.cooldown_secs });
        if (next.looted?.length) {
          setLootToast({ id: `instant-${next.server_time}`, items: next.looted.filter((l) => !l.unknown) });
          const items = next.looted.filter((l) => !l.unknown).map((l) => `${l.quantity}× ${l.name}`).join(', ');
          return items ? t('activity_ui__farm_looted', { items }) : t('loot_table__no_loot_found');
        }
        // Fouille lancée : le décompte sous le bouton Farm le montre déjà, pas de second message.
        if (next.result === 'loot_table__loot_started') return null;
        return next.result ? t(next.result) : null;
      },
      setActionMessage,
    );
  const stopFarm = () =>
    void runAction(
      farmApi.stop,
      (next) => {
        setFarmState(next);
        return next.result ? t(next.result) : null;
      },
      setActionMessage,
    );
  const occupationElapsed = useCallback(() => {
    void Promise.all([loadCraft(), loadFarm(), loadCraftBook()]);
  }, [loadCraft, loadFarm, loadCraftBook]);

  const occupation = currentOccupation(travel, craftState, farmState);
  const can = availability(occupation, busy);
  // Pendant un voyage, la fenêtre reste ouverte (compte à rebours, arrêt) ; un craft ou un farm la
  // ferme, puisque partir est alors refusé. En mode admin, les fenêtres du joueur laissent la
  // place à celles de l'admin (Téléporter remplace Voyage).
  const travelWindow = !adminMode && (occupation === 'travel' || (travelOpen && occupation === null));
  const craftWindow = !adminMode && craftOpen && (occupation === null || occupation === 'craft');
  const inventoryWindow = !adminMode && inventoryOpen;

  const motion = usePlayerMotion(map, travel);
  const others = useMemo<OtherGroup[] | undefined>(() => {
    if (!map || !spectatorGroups) return undefined;
    const now = Math.floor(Date.now() / 1000);
    return spectatorGroups.flatMap((g) => {
      const m = motionOf(map, g.location, g.travel, now);
      return m ? [{ key: g.members.map((x) => x.user_id).join('-'), name: g.members.map((x) => x.name).join(', '), motion: m }] : [];
    });
  }, [map, spectatorGroups]);
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
  // Fin d'un import : le serveur a déjà relié la session à l'univers (`link_universe`). On relit
  // la session au lieu de refaire l'authentification, pendant que l'animation de naissance joue.
  const afterImport = (universeName: string) => {
    setImportOpen(false);
    setGenesis({ name: universeName });
    void (async () => {
      try {
        const fresh = await refreshSession();
        setState((s) => (s.kind === 'ready' ? { kind: 'ready', session: { ...s.session, ...fresh } } : s));
        setFogEnabled(fresh.fog_enabled);
        if (fresh.universe) await reloadAll();
      } catch {
        // Session expirée ou serveur redémarré : reconnexion complète (SDK réutilisé).
        setGenesis(null);
        void start();
      }
    })();
  };
  const genesisOverlay = genesis && (
    <UniverseGenesis t={t} universeName={genesis.name} mapReady={!!session.universe && map !== null} onFinished={() => setGenesis(null)} />
  );
  if (!session.universe)
    return session.can_import ? (
      <div className="import-page">
        {genesisOverlay}
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
    others,
    // Choisir un lieu sur la carte ouvre la fenêtre de voyage, si partir est permis.
    // En mode admin, la fiche du lieu à la place (pas de voyage en mode admin).
    onSelectPlace: (id: string | null) => {
      setSelected(id);
      if (adminMode) return void setPlaceSheetId(id);
      setLoreId(id);
      if (id && can.travel) setTravelOpen(true);
    },
  };
  // Mode admin : seulement si la session y a droit (le serveur revérifie chaque requête admin).
  const adminBar = adminMode && session.admin_mode_available;
  const actions = adminBar ? (
    <AdminActionBar
      t={t}
      open={adminWindow}
      canImport={session.can_import}
      canTeleport={session.access === 'admin'}
      canMusic={session.access === 'admin'}
      onMusic={() => {
        setMusicAdminOpen(true);
        setPanelOpen(false);
      }}
      onOpen={(w) => {
        setAdminWindow((cur) => (cur === w ? null : w));
        setPanelOpen(false);
      }}
      onImport={() => {
        setImportOpen(true);
        setPanelOpen(false);
      }}
    />
  ) : (
    <ActionBar
      t={t}
      occupation={occupation}
      busy={busy}
      craft={craftState?.craft ?? null}
      farm={farmState?.farm ?? null}
      message={actionMessage}
      travelOpen={travelWindow}
      craftOpen={craftWindow}
      inventoryOpen={inventoryWindow}
      onTravel={() => {
        setTravelOpen((o) => !o);
        setPanelOpen(false);
      }}
      onCraft={() => {
        const open = !craftOpen;
        setCraftOpen(open);
        setCraftMessage(null);
        if (open) {
          // Une fenêtre à la fois à cet endroit : l'inventaire laisse la place au livre.
          setInventoryOpen(false);
          setPanelOpen(false);
          void loadCraftBook('craft');
        }
      }}
      onInventory={() => {
        const open = !inventoryOpen;
        setInventoryOpen(open);
        if (open) {
          setCraftOpen(false);
          setPanelOpen(false);
          void loadCraftBook('inventory');
        }
      }}
      onFarm={startFarm}
      onStopCraft={stopCraft}
      onStopFarm={stopFarm}
      onElapsed={occupationElapsed}
    />
  );
  const panel = character ? (
    <CharacterPanel character={character} avatarUrl={avatar} t={t} actions={actions} onResync={() => void loadCharacter()} />
  ) : (
    // Sans personnage, un admin garde sa barre d'actions (il n'a pas besoin de jouer).
    <aside className="side-panel">
      {characterError && characterError !== 'activity__no_character' ? (
        <p className="side-panel-message">{t(characterError)}</p>
      ) : (
        session.universe && !adminBar && (
          <Newcomer
            t={t}
            spectator={spectating}
            onSpectate={(on) => {
              setSpectatorChoice(on);
              // Rôle spectateur du serveur, s'il est configuré (sans effet sinon).
              if (on) void newcomerApi.spectate().catch(() => undefined);
            }}
          />
        )
      )}
      {adminBar && actions}
    </aside>
  );
  const open = layout === 'wide' ? columnOpen : panelOpen;
  const togglePanel = () => (layout === 'wide' ? setColumnOpen((o) => !o) : setPanelOpen((o) => !o));
  return (
    <div className={`layout ${layout}${layout === 'wide' && !columnOpen ? ' column-closed' : ''}`}>
      {genesisOverlay}
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
          onToggleAdmin={() => {
            // Changer de mode ferme les fenêtres de l'autre mode.
            setAdminMode((on) => !on);
            setAdminWindow(null);
            setTravelOpen(false);
            setCraftOpen(false);
            setInventoryOpen(false);
          }}
          onOpenSettings={() => setSettingsOpen(true)}
          t={t}
        />
        <div className="map-view">
          {map ? (
            // Export Azgaar (son image) ou sauvegarde Azgaar (rendu façon Azgaar) : en 2D. Sinon :
            // scène 3D, ou 2D sans WebGL.
            map.background || map.style || !use3d ? (
              <MapView2D map={map} {...mapProps} t={t} />
            ) : (
              <Suspense fallback={<MapView2D map={map} {...mapProps} t={t} />}>
                <MapScene map={map} {...mapProps} />
              </Suspense>
            )
          ) : (
            <div className="map-placeholder">{t('activity_ui__map')}</div>
          )}
          {craftWindow && (
            <CraftWindow
              t={t}
              book={book}
              canStart={can.startCraft}
              busy={busy}
              message={craftMessage}
              onStart={startCraft}
              onClose={() => setCraftOpen(false)}
            />
          )}
          {inventoryWindow && (
            <InventoryWindow
              t={t}
              inventory={inventory}
              onUse={async (itemId) => {
                try {
                  const r = await craftApi.useItem(itemId);
                  setInventory(r.inventory);
                  // Effets appliqués : fiche (statuts), vitesse et voyage relus aussitôt.
                  void Promise.all([loadCharacter(), loadTravel()]);
                  return t(r.result, { item_name: r.item_name });
                } catch (e) {
                  return t(errorKey(e));
                }
              }}
              onClose={() => setInventoryOpen(false)}
            />
          )}
          {adminBar && adminWindow && (
            <Suspense fallback={null}>
              {adminWindow === 'teleport' && <TeleportWindow t={t} onClose={() => setAdminWindow(null)} />}
              {adminWindow === 'items' && <ItemEditor t={t} onClose={() => setAdminWindow(null)} />}
              {adminWindow === 'recipes' && <RecipeEditor t={t} onClose={() => setAdminWindow(null)} />}
            </Suspense>
          )}
          {travel && travelWindow && (
            <TravelPanel
              state={travel}
              selectedId={selected}
              busy={busy}
              message={message}
              speed={travel.group_speed_kmh ?? character?.speed ?? null}
              compact={layout === 'compact'}
              t={t}
              onStart={(id) => void act(() => travelApi.start(id))}
              onStop={() => void act(travelApi.stop)}
              onLeave={() => void act(travelApi.leave)}
              onJoin={(leaderId) => void act(() => travelApi.join(leaderId))}
              onInvite={(userId) => void act(() => travelApi.invite(userId))}
              onAcceptInvite={(fromId) => void act(() => travelApi.acceptInvite(fromId))}
              onDeclineInvite={(fromId) => void act(() => travelApi.declineInvite(fromId))}
              onElapsed={() => {
                void Promise.all([loadTravel(), loadCharacter()]);
                setMusicVersion((v) => v + 1);
              }}
              onClose={() => setTravelOpen(false)}
            />
          )}
          {adminMode && placeSheetId && (
            <PlaceSheet placeId={placeSheetId} t={t} locale={uiLocale} version={placeVersion} onClose={() => setPlaceSheetId(null)} />
          )}
          {!adminMode && loreId && <PlaceLore placeId={loreId} t={t} locale={uiLocale} onClose={() => setLoreId(null)} />}
          <MusicControl t={t} version={musicVersion} />
          {lootToast && <LootToast toast={lootToast} t={t} onClose={closeLootToast} />}
        </div>
      </main>
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
