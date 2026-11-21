import type { ImportMode, ImportPlan, JobState, PlanRequest, RemovalChoices, RemovalInfo, ServerChoice, SetupType, UploadedMap } from '../api';

/** Choix de l'admin dans l'écran d'import. */
export interface ImportChoices {
  mode: ImportMode;
  includeSea: boolean;
  /** Routes secrètes : clés `a-b` avec a < b (identifiants de bourgs). */
  secret: Set<string>;
  /** Rattachements « Créer en plus » : bourg → lieu existant. */
  attach: Map<number, string>;
  /** Serveur choisi pour chaque emplacement après le premier (celui de la session). */
  extraServers: (string | null)[];
  setup: SetupType;
  /** Suppression des lieux existants : lieu de départ (bourg de la nouvelle carte). */
  startBurg: number | null;
  /** Objets posés : action choisie (par défaut : rendre au propriétaire). */
  itemActions: Map<string, 'destroy' | 'return'>;
  /** Q2 : destruction des objets sans propriétaire confirmée. */
  confirmUnowned: boolean;
}

export const roadKey = (a: number, b: number) => (a < b ? `${a}-${b}` : `${b}-${a}`);

/** Choix de départ : remplacer s'il n'y a pas d'univers, sinon créer en plus avec les
 * rattachements exacts proposés par le serveur. */
export function initialChoices(map: UploadedMap): ImportChoices {
  const attach = new Map<number, string>();
  for (const s of map.suggestions) if (s.status === 'exact' && s.burgs.length === 1) attach.set(s.burgs[0], s.place_id);
  return {
    mode: map.has_universe ? 'merge' : 'replace', includeSea: false, secret: new Set(), attach, extraServers: [], setup: 'FullSetup',
    startBurg: null, itemActions: new Map(), confirmUnowned: false,
  };
}

export function toggleSecret(secret: Set<string>, a: number, b: number): Set<string> {
  const next = new Set(secret);
  const k = roadKey(a, b);
  if (next.has(k)) next.delete(k);
  else next.add(k);
  return next;
}

/** Requête d'aperçu : seuls les emplacements remplis sont envoyés, sans doublon. */
export function planRequest(c: ImportChoices): PlanRequest {
  const servers: string[] = [];
  for (const s of c.extraServers) if (s && !servers.includes(s)) servers.push(s);
  return {
    mode: c.mode,
    include_sea: c.includeSea,
    secret: [...c.secret].map((k) => k.split('-').map(Number) as [number, number]),
    attach: c.mode === 'merge' ? [...c.attach].map(([burg_id, place_id]) => ({ burg_id, place_id })) : [],
    servers,
    setup: c.setup,
  };
}

/** Emplacements à afficher : autant que de serveurs dans le plan (au moins ceux déjà choisis). */
export function slotCount(plan: ImportPlan | null, c: ImportChoices): number {
  return Math.max(plan?.servers.length ?? 1, 1 + c.extraServers.length);
}

/** Serveurs proposés pour l'emplacement `slot` (≥ 1) : utilisables, ni le serveur courant, ni un
 * serveur déjà pris par un autre emplacement. */
export function optionsForSlot(servers: ServerChoice[], c: ImportChoices, slot: number): ServerChoice[] {
  const taken = new Set(c.extraServers.filter((s, i) => s && i !== slot - 1));
  return servers.filter((s) => s.eligible && !s.current && !taken.has(s.guild_id));
}

export function setSlot(c: ImportChoices, slot: number, guildId: string | null): ImportChoices {
  const extra = [...c.extraServers];
  while (extra.length < slot) extra.push(null);
  extra[slot - 1] = guildId;
  // Les emplacements vides en fin de liste disparaissent : le plan dira s'il en faut plus.
  while (extra.length && extra[extra.length - 1] === null) extra.pop();
  return { ...c, extraServers: extra };
}

/** Q11 : pas d'import tant qu'un serveur nécessaire manque. */
export function missingServers(plan: ImportPlan | null): number {
  return plan ? plan.servers.filter((s) => s.guild_id === null).length : 0;
}

/** Ce qui empêche de valider (clé de traduction), ou `null`. */
export function blocker(plan: ImportPlan | null, removal: RemovalInfo | null = null, c: ImportChoices | null = null): string | null {
  if (!plan) return 'activity_ui__import_wait_preview';
  if (missingServers(plan) > 0) return 'activity_ui__import_servers_missing';
  if (removal && removal.characters > 0 && (c?.startBurg == null || !plan.places.some((p) => p.burg_id === c.startBurg)))
    return 'activity__import_start_place_required';
  if (removal && removal.items.some((i) => i.owner === null) && !c?.confirmUnowned) return 'activity__import_confirm_unowned';
  return null;
}

/** Choix de suppression envoyés au serveur (I15, Q2). */
export function removalChoices(c: ImportChoices): RemovalChoices {
  return {
    start_burg: c.startBurg ?? undefined,
    items: [...c.itemActions].map(([tool_id, action]) => ({ tool_id, action })),
    confirm_destroy_unowned: c.confirmUnowned,
  };
}

/** I12 : le nom saisi doit être exactement le nom attendu (espaces de bord ignorés). */
export function nameConfirmed(expected: string, typed: string): boolean {
  return expected.trim().length > 0 && expected.trim() === typed.trim();
}

export interface Summary {
  createPlaces: number;
  keptPlaces: number;
  createRoads: number;
  updatedRoads: number;
  servers: number;
  cutRoads: number;
  isolated: number;
}

export function summary(plan: ImportPlan): Summary {
  return {
    createPlaces: plan.places.filter((p) => p.action.kind === 'create').length,
    keptPlaces: plan.places.filter((p) => p.action.kind !== 'create').length,
    createRoads: plan.roads.filter((r) => r.action.kind === 'create').length,
    updatedRoads: plan.roads.filter((r) => r.action.kind === 'update').length,
    servers: plan.servers.length,
    cutRoads: plan.cut_roads,
    isolated: plan.isolated.length,
  };
}

/** Durée estimée : environ une demi-seconde par objet Discord (mesuré : 0,35 à 0,45 s). */
export function estimatedMinutes(discordObjects: number): number {
  return Math.max(1, Math.ceil((discordObjects * 0.5) / 60));
}

/** Avancement en %, pour la création comme pour le retour en arrière. */
export function percent(job: JobState['progress']): number {
  if (!job.total) return job.phase === 'Done' || job.phase === 'RolledBack' ? 100 : 0;
  return Math.min(100, Math.round((100 * job.done) / job.total));
}

/** Couleur d'un serveur dans l'aperçu (palette stable, assez contrastée sur fond sombre). */
const PALETTE = ['#5aa9e6', '#f4a259', '#7fc8a9', '#e56b6f', '#b39ddb', '#f2d16b', '#4dd0e1', '#ff8fab', '#a3c45a', '#c9ada7'];
export const serverColor = (i: number) => PALETTE[i % PALETTE.length];
