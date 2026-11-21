# Activité Discord VerseEngine — Architecture

> Statut : **v2** (branche `feature/discord-activity`). Les décisions de l'utilisateur du 18/11/2026 sont intégrées (§ 13).

## 1. Objectif et périmètre

Une Activité Discord (Embedded App SDK) lancée depuis un salon vocal d'un serveur rattaché à un univers VerseEngine. Elle offre :

| Phase | Contenu | État |
|------:|---------|------|
| a | Squelette activité, authentification OAuth2, lecture du personnage (menu gauche) | **livré (back + front minimal)** |
| b | Carte « 3D » (Azgaar ou placement calculé), déplacements, compte à rebours temps réel | logique pure livrée (placement + import Azgaar), API/vue à faire |
| c | Inventaire : consulter / utiliser / consommer / équiper / déséquiper | à faire (équipement = simple état, sans emplacement — D6) |
| d | Mode admin : lieux, objets, validation des personnages, logs, carte des joueurs | à faire (décisions D3, D4, D9 prises) |

Hors périmètre : abonnements (seulement l'emplacement du bouton), PNJ IA, tout élément « premium ».

## 2. Constats sur le code existant (revérifiés le 18/11/2026 sur `main@a47fe8d`)

- **Aucune couche HTTP** : le binaire ne fait que `connect_db` → `constraint` → `connect_bot`.
- **Ordonnanceurs en mémoire** : `travel::logic::{MOVES, SLEEPER}`, idem `craft::logic`, `loot::logic`, rechargés depuis Mongo dans `Handler::ready`. Toute action de voyage doit passer par `add_travel` / `stop_travel` / `remove_move` **dans le même processus**.
- **La logique slash est mêlée au `poise::Context`** : `travel/start.rs::move_from_place/move_from_road` prennent un `&serenity::Context` mais n'utilisent que `ctx.http` ; `join.rs`/`leave.rs` construisent la logique directement dans la commande. → En phase b, ces fonctions seront extraites dans `travel/service.rs` (signature `Arc<Http>` + ids), appelées à la fois par les commandes slash et par l'API. **Aucune duplication.**
- Les **routes référencent les lieux par `category_id` Discord** (`place_one_id`, `place_two_id` = `u64`), pas par `ObjectId`. Toute l'API carte utilise donc `category_id` comme identifiant de lieu (sérialisé en chaîne, cf. § 5).
- `Place` : aucune coordonnée. `Road` : `distance` (km), `secret`. Depuis Q1 (`IMPORT_SPEC.md`), une route n'a plus de rôle : l'accès à son salon est une permission individuelle posée au départ et retirée à l'arrivée.
- `Character` : ni description ni image. La description n'existe que dans l'embed de la fiche (champ n°0). Les fiches **en attente** ne sont **pas en base** : elles n'existent que comme messages Discord avec boutons (`create_character__accept_character`…). L'acceptation lit l'embed (titre = nom, footer = user id).
- `ItemUsage::Wearable` existe, sans état « équipé ».
- Logs : uniquement des messages dans `log_channel_id`.
- `Server.admin_role_id` / `moderator_role_id` (`Option<Id>`), et `verify_moderator_permission` accepte aussi la permission Discord ADMINISTRATOR.
- `Cargo.lock` était dans `.gitignore` : il est désormais versionné (D11), `cargo test --locked` fonctionne sur un clone neuf.
- Les tests existants `test_connect_db` et `test_discord_bot_connection` exigent une base et un token : ils échouent dans un environnement sans secrets. Les nouveaux tests n'ont besoin de rien.
- Remarque annexe : `handle_accept_character_modal` construit `Character { _id: Default::default() }` (= `ObjectId::new()`) puis fait un `replace_one` en upsert : si une fiche existe déjà pour ce joueur, Mongo refusera la modification de `_id`. Ré-accepter un personnage existant échoue donc. Non corrigé ici (hors périmètre), signalé.

## 3. Vue d'ensemble

```
┌──────────────── Client Discord (iframe, CSP stricte) ───────────────┐
│  web/activity  (TypeScript + Vite + React + three.js/r3f)           │
│   DiscordSDK.ready → authorize(code) ──┐   fetch /api/*  ws /api/ws │
└────────────────────────────────────────┼────────────────────────────┘
                                         │  https://<client_id>.discordsays.com/.proxy/…
                                  Discord Proxy (URL mapping « / » → activity.<domaine>)
                                         │
                                 Reverse proxy TLS (Caddy)
                                         │ http://127.0.0.1:8787
┌──────────────────── Processus unique RpBot_reborn ──────────────────┐
│  axum (src/activity/*)  ── appelle ──►  services partagés           │
│     auth, sessions, droits,             travel::logic (MOVES…)      │
│     rate-limit, DTO, WS hub             database::*                 │
│  poise/serenity (bot)    ── appelle ──►  mêmes services             │
│  change streams Mongo ──► hub d'évènements ──► WebSocket clients    │
└───────────────────────────────────┬─────────────────────────────────┘
                                    │
                            MongoDB (replica set)
```

### Pourquoi l'API dans le même processus ?

- Les files `MOVES`/`SLEEPER` sont des statiques du processus : un second processus qui écrirait directement en base laisserait le bot ignorer le voyage jusqu'au prochain redémarrage.
- Alternative étudiée : processus API séparé + **file de commandes** en base consommée par le bot via change stream. Avantage : isolement des pannes et scalabilité horizontale de l'API. Inconvénients : latence, réponse asynchrone (l'API ne sait pas immédiatement si `travel__no_road_found`…), double déploiement, complexité de corrélation. Non retenue pour la v1 ; le découpage en `services` rend la migration possible plus tard (l'API n'appelle que des fonctions sans `poise::Context`).
- L'API est **désactivée par défaut** : elle ne démarre que si `ACTIVITY_ENABLED=true`. Un plantage d'une route axum ne fait pas tomber le bot (tâche tokio séparée, handlers sans `unwrap`).

## 4. Stack front et justification

| Choix | Raison |
|-------|--------|
| **TypeScript + Vite** | build rapide, sortie statique (pas de serveur Node en prod), typage des DTO partagés. |
| **React 18** | menu gauche, inventaire et formulaires admin sont des interfaces à états nombreux ; React est le choix le plus outillé. |
| **three.js via @react-three/fiber** (+ `drei` en import ciblé) | la carte est une scène simple : un maillage de terrain (heightmap Azgaar sous-échantillonnée ≤ 256×256), des marqueurs instanciés (`InstancedMesh`) pour lieux et joueurs, des lignes pour les routes. r3f permet d'écrire la scène dans le même modèle React que l'UI. |
| `@discord/embedded-app-sdk` | obligatoire pour `authorize`/`authenticate` et `instanceId`. |
| i18n : **@fluent/bundle** | **source unique** : le front charge `translations/{fr,en-US}.ftl` du bot via `GET /api/i18n/{locale}` (liste blanche). Les clés d'interface sont préfixées `activity_ui__`, les erreurs `activity__` (ou les clés existantes du bot, ex. `travel__no_road_found`). Un test vérifie que chaque clé `activity__*` du code existe dans les deux langues. Langue : `userSettingsGetLocale()` du SDK, repli `fr`. |

Performances (laptop modeste, iframe) : `frameloop="demand"` (rendu seulement sur interaction ou animation), DPR plafonné à 1.5, terrain en un seul `BufferGeometry` sans ombres temps réel, éclairage hémisphérique, code-splitting du mode admin (`React.lazy`). Le compte à rebours est un composant DOM à 1 Hz, pas un rendu 3D.

Repli : si WebGL indisponible, la carte bascule en vue 2D SVG (mêmes données).

## 5. API HTTP (préfixe `/api`, JSON)

Conventions :
- **Tous les identifiants Discord (snowflakes) et `ObjectId` sont des chaînes** (pas de perte de précision JS).
- Erreurs : `{"error": "<clé_fluent>"}` + statut HTTP. Les clés réutilisent celles du bot (`travel__no_road_found`…) pour que le front affiche le même texte que la commande slash.
- Authentification : `Authorization: Bearer <session_token>` (pas de cookie : l'iframe est un contexte tiers, les cookies y sont peu fiables, et un Bearer évite le CSRF).

| Méthode | Route | Phase | Rôle |
|--------|-------|:----:|------|
| POST | `/api/auth/token` | a | `{code, instance_id}` → `{access_token, session_token, expires_in, user, universe, is_admin}` |
| GET | `/api/me` | a | session courante : utilisateur, univers, serveur, droits |
| GET | `/api/me/character` | a | personnage : nom, description, image, stats résolues (dont vitesse), modificateurs actifs, position/voyage |
| GET | `/api/map` | b | lieux (+ positions), routes non secrètes (+ route courante si secrète), source du placement, terrain éventuel |
| GET | `/api/me/travel` | b | état du voyage : `step_end_timestamp`, `eta_timestamp`, distance restante, vitesse, membres |
| POST | `/api/me/travel/start` | b | `{destination_id}` → même service que `/travel_start` |
| POST | `/api/me/travel/stop` | b | même service que l'arrêt de `/travel_start` (chef de groupe) |
| POST | `/api/me/travel/join` | b | `{target_user_id}` → même service que `/travel_join` |
| POST | `/api/me/travel/leave` | b | même service que `/travel_leave` |
| GET | `/api/me/inventory` | c | objets, quantités, état équipé |
| POST | `/api/me/inventory/{inventory_id}/use` · `/consume` · `/equip` · `/unequip` | c | mêmes services que `/item use`, `/item consume` |
| GET | `/api/ws` | a | WebSocket (cf. § 7) |
| GET | `/api/i18n/{locale}` | a | fichier Fluent du bot (`fr`, `en-US`), public |
| GET | `/api/admin/import/servers` | d | serveurs utilisables pour l'import (voir `IMPORT_SPEC.md` § 13) |
| POST | `/api/admin/import/map` | d | sauvegarde `.map` d'Azgaar, 32 Mio au plus |
| POST | `/api/admin/import/plan` | d | aperçu de l'import |
| POST | `/api/admin/import/execute` | d | création, après saisie du nom de l'univers |
| GET | `/api/admin/import/jobs/{id}` | d | progression d'un import |
| — | `/api/admin/*` | d | lieux, objets, fiches en attente, logs, positions des joueurs |

Validation des entrées : désérialisation stricte (`deny_unknown_fields`), ids parsés (`u64`/`ObjectId`) avant tout accès base, longueurs bornées (noms ≤ 100, descriptions ≤ 4000), corps limité à 64 Kio (import Azgaar : route admin dédiée à 32 Mio, traitée en flux).

**Calcul du temps restant** (`eta_timestamp`) : `step_end_timestamp` n'est l'arrivée que si `is_end = true`. Sinon, l'étape s'arrête à l'expiration d'un modificateur de vitesse ; le serveur calcule alors `eta = now + (distance_restante / modified_speed) × 3600` avec `distance_restante = road.distance − calculate_current_distance()` (fonction existante), et renvoie aussi `step_end_timestamp` pour que le client sache quand se resynchroniser. Le client décompte localement à partir de `eta_timestamp − (server_now − client_now)` (décalage d'horloge mesuré via le champ `server_time` de chaque réponse/évènement).

## 6. Authentification et autorisation

```
Client (iframe)                     Serveur VerseEngine                     Discord
──────────────                      ───────────────────                     ───────
new DiscordSDK(CLIENT_ID)
instance_id = sdk.instanceId
await sdk.ready()
code = sdk.commands.authorize({client_id, response_type:"code",
        prompt:"none", scope:["identify"]})
POST /api/auth/token {code, instance_id} ─►
                                     POST /api/oauth2/token (client_secret) ─► access_token
                                     GET /users/@me (Bearer access_token)   ─► user.id
                                     GET /applications/{app}/activity-instances/{instance_id}
                                         (Bot token)                        ─► location.guild_id, users[]
                                     ✔ user.id ∈ users
                                     ✔ guild_id → Server → universe_id
                                     ✔ membre de la guilde (bot) → rôles → is_admin
                                     session_token = 32 octets aléatoires (CSPRNG)
             ◄── {access_token, session_token, …}
sdk.commands.authenticate({access_token})
```

Règles :
- **Le client secret et le token bot ne quittent jamais le serveur** ; lus depuis `DISCORD_CLIENT_SECRET` / `DISCORD_TOKEN`, jamais journalisés (`Debug` masqué).
- **Le serveur ne croit jamais le client** sur l'identité, la guilde ou les droits : la guilde vient de l'API *activity-instances* de Discord (authentifiée par le bot), l'utilisateur de `/users/@me`, les droits des rôles du membre lus par le bot comparés à `Server.admin_role_id` / `moderator_role_id` (+ permission ADMINISTRATOR, comme `verify_moderator_permission`).
- La session est liée à `(user_id, guild_id, universe_id, instance_id)`, expire après 1 h (renouvelée par un nouvel `authorize` silencieux, `prompt:"none"`). Stockée **hachée (SHA-256)** dans une table en mémoire ; seule l'empreinte est conservée.
- Les droits admin sont **revérifiés** à chaque requête `/api/admin/*` (cache des rôles 30 s) pour qu'un retrait de rôle prenne effet vite.
- Le scope OAuth demandé est minimal : `identify` (pas besoin de `guilds` : l'appartenance est vérifiée par le bot).
- Le `access_token` Discord n'est pas conservé côté serveur au-delà de l'échange.

Limitation de débit (en mémoire, seau à jetons, `src/activity/ratelimit.rs`) :
- `/api/auth/token` : rafale 10, puis 10 req/min par instance d'activité — **livré** ;
- routes authentifiées et messages WebSocket : rafale 20, 5 req/s soutenu, par utilisateur — **livré** ;
- actions d'écriture (voyage, inventaire, admin) : 1 req/s soutenu par utilisateur — phase b ;
- limite globale par IP : peu pertinente derrière le proxy Discord (qui masque l'IP) ; à faire au niveau de Caddy si besoin.
Réponse `429` avec `Retry-After`.

En-têtes : `Content-Security-Policy` stricte sur les fichiers statiques (`default-src 'self'`), `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`. Azgaar (`/azgaar/`, étape E9 de l'import) est servi avec `Content-Security-Policy: sandbox …` : origine opaque, aucun accès à l'activité ni à l'API (voir `IMPORT_SPEC.md` § 18). CORS : aucune origine externe (tout passe par le proxy Discord, même origine).

## 7. Temps réel

- **WebSocket** `/api/ws` (le proxy Discord ne supporte que WebSocket ; SSE fonctionne mais WS est la voie documentée). Le jeton de session est passé en premier message `{"type":"auth","token":…}` plutôt qu'en query string pour ne pas finir dans des logs.
- **Source des évènements : change streams MongoDB** sur `travels`, `characters`, `inventory` (et `places`, `roads`, `universe_maps` pour l'admin), filtrés par `universe_id`. Avantage décisif : une action faite par commande slash est poussée exactement comme une action de l'activité, **sans modifier `travel/logic.rs`** ni les commandes. Un seul change stream par collection pour tout le processus, réparti par un `tokio::sync::broadcast` vers les sockets abonnées (filtrage par univers puis par droits : un joueur ne reçoit que son groupe ; l'admin reçoit tous les joueurs de l'univers).
- Reprise : le `resume_token` est conservé en mémoire ; en cas de coupure, on rouvre le stream depuis ce token, sinon le client reçoit `{"type":"resync"}` et recharge par HTTP.
- Battement de cœur 25 s ; le client se reconnecte avec back-off exponentiel (1 → 30 s) et refait un `GET` complet à la reconnexion.
- Le compte à rebours est calculé côté client (cf. § 5) ; un évènement `travel_updated` arrive à chaque changement d'étape (`next_step_logic` fait un upsert) et à l'arrivée.

## 8. Modèle de données ajouté

Principe : **ajouts optionnels et rétro-compatibles** (`#[serde(default)]`), dans des fichiers séparés autant que possible.

### 8.1 `Character` (modification minimale)
```rust
#[serde(default)] pub description: Option<String>, // copiée depuis l'embed à l'acceptation
#[serde(default)] pub image: Option<CharacterImage>, // {channel_id, message_id, attachment_id} — D5
```
Les documents existants se lisent sans erreur (`None`). La description est renseignée au moment de l'acceptation (`handle_accept_character_modal`) à partir du premier champ de l'embed, ce qui ne change rien au flux Discord.

### 8.2 Nouvelle collection `universe_maps` (plutôt qu'un champ sur `Place`)
```jsonc
{ "_id": ObjectId, "universe_id": ObjectId,
  "source": "Azgaar" | "Computed" | "Manual",
  "width": 1000.0, "height": 1000.0,
  "positions": [ { "place_id": "<category_id>", "x": 12.3, "y": 45.6, "z": 0.4, "azgaar_burg_id": 17 } ],
  "terrain": { "cols": 128, "rows": 96, "heights": [u8…] } | null,  // ≤ 64 Kio
  "updated_at": 1763459000 }
```
Pourquoi pas `Place.position` ? (1) le terrain (heightmap) est lié à la carte, pas aux lieux ; (2) un import Azgaar remplace toutes les positions d'un coup, atomiquement ; (3) aucun conflit avec les correctifs en attente ; (4) `Place` est construit en dur à plusieurs endroits. On y ajoute la zone explorable : `zone: { polygon: [[x,y]…], outside: "crop" | "grey" | "remove" }` (D7).

### 8.3 Collections à venir
- `character_submissions` (phase d, **validé D4**) : fiches en cours de création / en attente, écrites **par le flux Discord existant et par l'activité** (le message Discord garde l'embed et les boutons, mais lit/écrit la base). Champs : `universe_id, user_id, name, description, story, special_request, image?, status (draft|submitted|accepted|refused), message_ref, created_at, updated_at`.
- `Inventory.equipped: bool` (`#[serde(default)]`), **sans emplacement** (D6). Les effets de l'équipement passeront plus tard par des « champs de valeurs » ; en phase c, équiper/déséquiper ne fait que basculer l'état et journaliser.
- `known_roads` (phase b, D10, **livré**) : `{universe_id, user_id, road_ids: [channel_id…]}`, index unique `(universe_id, user_id)`, `$addToSet` à chaque départ sur une route, **dans le service de voyage partagé** (donc identique slash/activité).
- `logs` (D9) : `{universe_id, server_id, ts, actor_id, kind, target, payload}`. `kind` couvre **tout ce qui modifie le gameplay** : consommation, équipement/déséquipement, échange, utilisation d'objet, loot, craft (début/fin/arrêt), déplacement (départ/arrêt/arrivée/rejoindre/quitter), début/fin d'effet, lieux, routes, objets, loot tables, recettes (création/modification/suppression), validation/refus de fiche. Une fonction unique `gameplay_log::record()` écrit en base (dans la transaction de l'action quand il y en a une) **et** envoie au salon de log. Rétention : pas de TTL par défaut (à confirmer si le volume devient un problème).

## 9. Carte

### Source A — Azgaar Fantasy Map Generator
Format retenu : **Export → JSON → Full** (`.json`). Utilisé : `info` (`width`, `height`, `mapName`), `settings.distanceScale` / `distanceUnit` (échelle), `pack.burgs[]` (`i`, `name`, `x`, `y`, `cell`, `removed`, `capital`), `pack.cells` (`h` hauteur 0–100, ≥ 20 = terre ; `p` = `[x,y]`), `pack.routes[]` (`points [[x,y,cell]…]`, `group`). Le format `.map` (texte propriétaire) n'est pas utilisé : il est plus fragile et non documenté.

Import (route admin, phase d — la logique pure est **déjà livrée** dans `src/activity/map/azgaar.rs`) :
1. parse tolérant (champs inconnus ignorés ; `cells` accepté sous forme de tableau d'objets ou d'objet de tableaux) ;
2. **zone explorable** dessinée par l'admin (polygone) ; l'extérieur est recadré, grisé ou effacé au choix (D7). Burgs et tronçons hors zone ne sont jamais importés (`network::Zone`) ;
3. **rattachement burg → lieu** par nom normalisé, exact puis préfixe ; l'admin confirme/corrige. Les burgs retenus sans lieu correspondant peuvent être **créés** comme lieux (catégorie + rôle Discord créés par la même logique que `/place create`) ;
3. heightmap rééchantillonnée sur une grille ≤ 128×128 (u8) à partir des centres de cellules ;
4. écriture atomique de `universe_maps` en transaction.
**Distances des routes (D8)** — livré dans `src/activity/map/network.rs` : les routes Azgaar sont des polylignes qui se croisent ; une bifurcation = cellule partagée. Pour chaque burg retenu, Dijkstra sur le graphe des tronçons **s'arrête au premier burg retenu rencontré** : on obtient les paires de lieux voisins et leur distance, **somme des tronçons** à travers les bifurcations et les burgs non retenus. Poids d'un tronçon = longueur en pixels × `distanceScale`, ou longueur déclarée de la route répartie au prorata ; conversion mi/lieues → km. Les routes maritimes sont exclues par défaut, ou proposées comme navettes (`uses_sea`). L'admin valide la liste avant création des routes.

### Source B — placement calculé (déjà livré, `src/activity/map/layout.rs`)
1. Graphe lieux/routes (non secrètes et secrètes, côté serveur seulement), poids = `distance`.
2. Plus courts chemins (Dijkstra) → matrice de distances ; composantes non connexes placées côte à côte.
3. **MDS classique** (double centrage + puissance itérée, 2 vecteurs propres) pour une position initiale déterministe.
4. **Raffinement par stress majorization (SMACOF)**, pondéré `1/d²`, 300 itérations max : minimise `Σ w_ij (‖x_i − x_j‖ − d_ij)²`, ce qui respecte au mieux les longueurs des routes.
5. Normalisation dans `[0,1000]²` et score de stress renvoyé (affiché à l'admin).
Déterministe (pas d'aléa) : la même base donne la même carte. Coût O(n²) mémoire, adapté jusqu'à ~2 000 lieux ; résultat mis en cache dans `universe_maps` (`source: "Computed"`) et recalculé quand une route change.

### Rendu
Terrain extrudé (hauteur exagérée, eau à plat), lieux en piliers/icônes, routes en tubes fins ; position d'un groupe en voyage = interpolation sur le segment de la route selon `distance_traveled` + vitesse, mise à jour à 1 Hz. Clic sur un lieu voisin → panneau « Partir vers … (durée estimée) ». Les **routes secrètes** ne sont envoyées à un joueur que s'il les a déjà empruntées (`known_roads`, D10) ou s'il est dessus.

## 10. Hébergement

Procédure complète, vérifiée sur la documentation officielle : **[`DEPLOY.md`](DEPLOY.md)**.

- Le binaire écoute `ACTIVITY_BIND` (défaut `127.0.0.1:8787` ; `0.0.0.0:8787` dans le conteneur, publié sur `127.0.0.1` seulement) et sert le front statique (`ACTIVITY_STATIC_DIR`, défaut `web/activity/dist`).
- TLS par un reverse proxy (Caddy) sur le sous-domaine ; URL mapping Discord `/` → ce sous-domaine, sans protocole.
- `Dockerfile` multi-étapes : étape Node (`npm ci`, tests, build) puis `dist/` copié dans l'image du bot (D1). Aucun secret dans l'image.
- La commande Entry Point « Launch » créée par Discord est conservée lors de l'enregistrement des commandes du bot.

## 11. Tests

- `src/activity/map/layout.rs` : triangle et carré connus, chaîne linéaire, composantes disjointes, graphe vide, stress décroissant, déterminisme.
- `src/activity/map/azgaar.rs` : parsing d'un extrait JSON Full, burgs retirés ignorés, normalisation des noms, rattachement exact/ambigu, rééchantillonnage du terrain.
- `src/activity/permissions.rs` : matrice rôles → niveau d'accès.
- `src/activity/ratelimit.rs` : seau à jetons (rafale, recharge, isolement des clés).
- `src/activity/session.rs` : jeton haché, expiration, révocation.
- `src/activity/travel_view.rs` : calcul de l'ETA (étape finale / étape intermédiaire / arrêt).
- `src/activity/http.rs` : routes axum testées avec `tower::ServiceExt::oneshot` (401 sans jeton, 401 jeton inconnu, 413 corps trop gros, validation de `code`).
- Lancer : `cargo test --locked activity::` (aucune base ni token requis).

## 12. Avancement (tenu à jour)

- [x] Document d'architecture et questions
- [x] Phase a — `src/activity/` : config (désactivée par défaut, secrets masqués), échange OAuth2 côté serveur, vérification instance → guilde → univers → membre → droits, sessions hachées, limitation de débit, `GET /api/me`, `GET /api/me/character`, WebSocket authentifié (battements), i18n servie par le bot, en-têtes de sécurité, front statique servi par axum
- [x] Phase a — `Character.description` / `image_url` (optionnels) ; la description est copiée de l'embed à l'acceptation
- [x] Phase a — front `web/activity` (Vite + React + TS) : auth SDK, menu gauche (portrait, description, statuts avec expiration, vitesse, position, compte à rebours), emplacement « Gérer mon abonnement », bascule admin visible seulement si le serveur l'autorise
- [x] Logique pure phase b : placement MDS + stress majorization, parseur Azgaar JSON Full + rattachement par nom + heightmap, vue ETA
- [x] Réseau routier Azgaar (somme à travers les bifurcations, routes maritimes optionnelles, longueurs déclarées, unités) et zone explorable (polygone, masque de terrain) — `map/network.rs`
- [x] `Character.image` = référence de pièce jointe (D5) ; `Cargo.lock` versionné (D11)
- [x] Phase b — `travel/service.rs` partagé (slash + activité), `known_roads`, `GET /api/map` (routes secrètes et lieux cachés filtrés côté serveur), `/api/me/travel*`, collection `universe_maps` (placement calculé mis en cache), change streams → WebSocket, `Character.image` → URL, scène three.js/r3f + repli 2D, Dockerfile multi-étapes, `DEPLOY.md`
- [ ] Phase c — inventaire (`equipped: bool`)
- [ ] Phase d — admin : `character_submissions`, image en paramètre + fil, `logs`, propositions, import Azgaar avec zone

## 13. Décisions de l'utilisateur (18/11/2026)

| # | Décision | Conséquence |
|---|----------|-------------|
| D1 | Même machine, **même conteneur Docker** que le bot. | Le binaire unique sert l'API et le front ; le `Dockerfile` gagne une étape Node ; le port 8787 est exposé au reverse proxy TLS. |
| D2 | Activité sur **l'application du bot**. | `DISCORD_CLIENT_ID` = id de l'application du bot ; `DISCORD_CLIENT_SECRET` à ajouter dans l'environnement. |
| D3 | **Modérateurs** : gèrent les joueurs (fiches, statuts, positions, logs) et valident/refusent les **propositions** (objets, crafts…), mais **n'éditent pas l'univers**. | `AccessLevel::Moderator` → mode admin restreint ; création/modification/suppression de lieux, routes, objets, loot tables, recettes, import Azgaar = `Admin` seulement. Un système de propositions (collection `proposals`) sera nécessaire : à concevoir en phase d. |
| D4 | Fiches en cours de création **stockées en base**. | Collection `character_submissions` (§ 8.3). |
| D5 | Image fournie **en paramètre de la commande** de création et stockée à ce moment ; sinon, un **fil** sur la fiche validée permet d'en joindre une ; la **suppression du message** (par le joueur ou un modérateur) retire l'image. | L'image est republiée dans la fiche (pièce jointe du message du bot), le personnage stocke la référence `{channel_id, message_id, attachment_id}` (les URL CDN Discord expirent, l'URL est régénérée à la lecture). Le gestionnaire `message_delete` du bot efface la référence. Le fil est créé à l'acceptation. |
| D6 | Pas d'emplacements d'équipement. | `equipped: bool`. Les effets viendront des futurs « champs de valeurs ». |
| D7 | Création de lieux possible à l'import ; l'admin indique la **zone explorable** (extérieur coupé, grisé ou supprimé au choix). | `universe_maps.zone` + `network::Zone` (livré). |
| D8 | Utiliser les distances des routes Azgaar en **additionnant à travers les bifurcations**. | `network::road_candidates` (livré, testé). |
| D9 | Journaliser **tout ce qui modifie le gameplay**. | Collection `logs` + `gameplay_log::record()` (§ 8.3). |
| D10 | Le joueur voit les routes secrètes **déjà empruntées**. | Collection `known_roads` alimentée par le service de voyage. |
| D11 | `Cargo.lock` versionné. | Fait. |
| D12 | Langue = **celle du client/joueur**. | Activité : langue du client Discord. Bot : les réponses éphémères utilisent déjà la langue de l'interaction ; les messages **publics** (arrivées, départs…) sont aujourd'hui en `fr` en dur — un message public est lu par plusieurs joueurs : proposition, utiliser la langue de la guilde (`guild.preferred_locale`) pour ceux-là, et la langue du joueur pour tout le reste (MP, éphémères, activité). |

### Accès en écriture au dépôt
Accordé le 18/11/2026. Les branches sont poussées sur CGBots/VerseEngine : `chore/hardening` (PR #11) et `feature/discord-activity` (PR #12, brouillon, contient #11 par fusion). Une nouvelle session obtient l'accès avec `add_repo` (accès `push`). Les livraisons par patch ne sont plus nécessaires.
