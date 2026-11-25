import type { CraftState, FarmState } from '../api';
import type { Translator } from '../i18n';
import { availability, type Occupation } from '../occupation';
import { Countdown } from './Countdown';
import { TimedProgress } from './TimedProgress';

/** Fenêtres du mode admin ouvertes depuis la barre d'actions. */
export type AdminWindow = 'recipes' | 'items' | 'teleport';

interface Props {
  t: Translator;
  occupation: Occupation;
  /** Une requête est en cours. */
  busy: boolean;
  craft: CraftState['craft'];
  farm: FarmState['farm'];
  message: string | null;
  travelOpen: boolean;
  craftOpen: boolean;
  inventoryOpen: boolean;
  onTravel: () => void;
  onCraft: () => void;
  onInventory: () => void;
  onFarm: () => void;
  onStopCraft: () => void;
  onStopFarm: () => void;
  /** Fin d'un craft ou d'un farm : relire l'état serveur. */
  onElapsed: () => void;
}

/**
 * Actions du panneau du personnage. Voyage, craft et farm s'excluent (règle du serveur) : ce qui
 * n'est pas permis est grisé, d'après l'état relu par l'API. L'inventaire se consulte toujours.
 * Voyage est tout en bas.
 */
export function ActionBar({ t, occupation, busy, craft, farm, message, travelOpen, craftOpen, inventoryOpen, onTravel, onCraft, onInventory, onFarm, onStopCraft, onStopFarm, onElapsed }: Props) {
  const can = availability(occupation, busy);
  return (
    <nav className="actions" aria-label={t('activity_ui__actions')}>
      <button type="button" className={`action${occupation === 'farm' ? ' active' : ''}`} disabled={!can.farm} onClick={onFarm}>
        {occupation === 'farm' ? t('activity_ui__farm_in_progress') : t('activity_ui__farm')}
      </button>
      {farm && (
        <div className="occupation-progress">
          <TimedProgress start={farm.start_timestamp} end={farm.end_timestamp} label={t('activity_ui__farm_in_progress')} onElapsed={onElapsed} />
          <div className="row">
            <span className="muted">{t('activity_ui__time_left')} <Countdown until={farm.end_timestamp} /></span>
            <button type="button" className="danger small" disabled={busy} onClick={onStopFarm}>{t('activity_ui__stop')}</button>
          </div>
        </div>
      )}

      <button
        type="button"
        className={`action${occupation === 'craft' ? ' active' : ''}`}
        disabled={!can.craft}
        aria-pressed={craftOpen}
        onClick={onCraft}
      >
        {t('activity_ui__craft')}
      </button>
      {craft && (
        <div className="occupation-progress">
          <p>{t('activity_ui__craft_progress', { recipe: craft.recipe_name ?? '?', quantity: craft.quantity })}</p>
          <TimedProgress start={craft.start_timestamp} end={craft.end_timestamp} label={t('activity_ui__craft_in_progress')} onElapsed={onElapsed} />
          <div className="row">
            <span className="muted">{t('activity_ui__time_left')} <Countdown until={craft.end_timestamp} /></span>
            <button type="button" className="danger small" disabled={busy} onClick={onStopCraft}>{t('activity_ui__stop')}</button>
          </div>
        </div>
      )}

      <button type="button" className="action secondary" disabled={!can.inventory} aria-pressed={inventoryOpen} onClick={onInventory}>
        {t('activity_ui__inventory')}
      </button>

      {message && <p className="action-message" role="status">{message}</p>}

      <button
        type="button"
        className={`action action-travel${occupation === 'travel' ? ' active' : ''}`}
        disabled={!can.travel}
        aria-pressed={travelOpen}
        onClick={onTravel}
      >
        {occupation === 'travel' ? t('activity_ui__travel_in_progress') : t('activity_ui__travel')}
      </button>
    </nav>
  );
}

interface AdminProps {
  t: Translator;
  /** Fenêtre admin ouverte. */
  open: AdminWindow | null;
  /** Import et génération de carte permis (créateur de l'univers ou administrateur). */
  canImport: boolean;
  /** Téléportation : administrateurs seulement, comme `/teleport` (le serveur revérifie). */
  canTeleport: boolean;
  /** Page « Musique » (bibliothèque et playlists) : administrateurs seulement. */
  canMusic: boolean;
  onOpen: (w: AdminWindow) => void;
  onImport: () => void;
  onMusic: () => void;
}

/**
 * Barre d'actions du mode admin : Recettes (éditeur, à la place de Craft), Objets (à la place
 * d'Inventaire), Carte (import), Musique, et Téléporter tout en bas, à la place de Voyage. Farm n'a pas
 * d'équivalent : l'admin gère l'univers, il ne joue pas. Le serveur revérifie les droits.
 */
export function AdminActionBar({ t, open, canImport, canTeleport, canMusic, onOpen, onImport, onMusic }: AdminProps) {
  const button = (w: AdminWindow, label: string, extra = '') => (
    <button type="button" className={`action${extra}`} aria-pressed={open === w} onClick={() => onOpen(w)}>
      {t(label)}
    </button>
  );
  return (
    <nav className="actions admin-actions" aria-label={t('activity_ui__admin_actions')}>
      {button('recipes', 'activity_ui__admin_recipes')}
      {button('items', 'activity_ui__admin_items', ' secondary')}
      {canImport && (
        <button type="button" className="action secondary" onClick={onImport}>
          {t('activity_ui__import_title')}
        </button>
      )}
      {canMusic && (
        <button type="button" className="action secondary" onClick={onMusic}>
          {t('activity_ui__music_admin_title')}
        </button>
      )}
      {canTeleport && button('teleport', 'activity_ui__admin_teleport', ' action-travel')}
    </nav>
  );
}
