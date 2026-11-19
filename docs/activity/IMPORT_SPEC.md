# Import et génération de carte — cahier des charges

Rédigé le 19/11/2026 à partir des réponses de l'utilisateur (toutes les questions sont tranchées). Sert de point de départ à une session dédiée. Complète `ARCHITECTURE.md` (décisions D1–D12) et `PROGRESS.md`.

## 1. Objectif

Dans la **vue admin** de l'activité, deux boutons :
- **« Importer une carte »** : import d'une sauvegarde Azgaar (`.map`), avec des instructions pour créer et sauvegarder une carte dans Azgaar ;
- **« Générer une map »** : carte simple générée par l'activité.

Dans les deux cas, on peut **valider** (double vérification) ou **annuler**. Après validation, les lieux, salons et routes sont **créés automatiquement**, avec les distances.

## 2. Décisions

| # | Sujet | Décision |
|---|-------|----------|
| I1 | Accès | Seuls le **créateur de l'univers** et les **administrateurs** voient et utilisent l'import et la génération. |
| I2 | Serveur sans univers | L'activité **doit se lancer** sur un serveur non relié à un univers. Les administrateurs du serveur peuvent alors créer l'univers par l'import ou la génération. |
| I3 | Azgaar dans Discord | On tente de faire tourner Azgaar dans l'activité (version figée, MIT, polices servies depuis notre domaine). **Si ce n'est pas possible** : redirection vers **azgaar.github.io**, puis import du `.map` sauvegardé. |
| I4 | Répartition sur plusieurs serveurs | **60 lieux au plus par serveur** (150 à l'origine, réduit par Q9), et respect des limites de Discord (§ 4). L'univers est **réparti sur plusieurs serveurs** en **minimisant les routes entre serveurs**. Avant la validation, l'admin voit **combien de serveurs** sont nécessaires, pour les créer. |
| I5 | Lieu | Comme `/place_create_place` (rôle + catégorie), **plus un salon textuel générique** valable pour tout type de lieu, par exemple `#place-centrale`. |
| I6 | États, provinces, régions d'Azgaar | **Ignorés** pour l'instant. |
| I7 | Routes | Une route par paire de lieux voisins, distance calculée sur le tracé Azgaar. **Routes maritimes : case « inclure »**, décochée par défaut. **Routes secrètes : un clic sur la route dans l'aperçu, puis une case « secrète »**. Les deux réglages sont indépendants. |
| I8 | Distances | **Aucune conversion ni multiplicateur** : la distance de la carte est conservée telle quelle, pour garder la cohérence spatiale. La durée des voyages se règle avec le modificateur de temps de l'univers et la vitesse des personnages. |
| I9 | Serveurs | Voir I4. Un menu permet d'indiquer les serveurs à utiliser. |
| I10 | Ré-import | **Ne rien recréer qui existe** : un lieu garde son bourg d'origine, et le ré-import met à jour positions et tracés, et ajoute ou retire ce qui a changé. |
| I11 | « Générer une map » | Une carte **simple**. Réglages : **nombre de lieux**, **graine affichée** (et regénération). Modifications possibles : **tracé des routes**, **ajout et suppression de routes et de lieux**, **renommage des lieux**. Pas de modification en profondeur (relief, biomes…). Rendu : la vue 3D détaillée actuelle. |
| I12 | Double vérification | Un écran récapitulatif, puis **la saisie du nom de l'univers**. |
| I13 | Progression et erreur | Barre de progression pendant la création. En cas d'erreur, **retour en arrière complet** (suppression de tout ce qui a été créé), avec sa propre barre de progression. |
| I14 | Univers absent | Mêmes informations que `/universe_create_universe` (nom, type d'installation), **plus la vitesse du temps en option**. Par défaut, temps réel (24 h par jour). |
| I15 | « Supprimer les lieux existants » | Les personnages présents sont déplacés vers un **lieu de départ** choisi dans la nouvelle carte. Les voyages en cours sont annulés. Les salons, catégories et rôles sont supprimés. Pour chaque **objet posé**, l'admin choisit de **le détruire** ou de **le rendre à son propriétaire, avec son contenu**. |
| I16 | « Créer en plus » | Les lieux existants dont le nom correspond à un bourg sont **rattachés** (déjà codé : `match_places`). Les autres bourgs retenus sont créés. |
| I17 | Brouillard | **Option propre à chaque univers**, au choix du créateur. |

## 3. Ce qui existe déjà (branche `feature/discord-activity`)

- Lecture du `.map` (`activity/map/azgaar_file.rs`, gzip / base64 / texte), testée sur une vraie sauvegarde (Moria, Azgaar 1.158) et comparée à son export JSON.
- Routes candidates entre bourgs voisins, avec leur **tracé** et une distance qui suit les bifurcations, sans les routes redondantes (`network.rs`, `prune_detours`). Zone explorable (`network::Zone`).
- Rattachement des bourgs aux lieux par nom (`azgaar::match_places`), relief (`resample_terrain`).
- `universe_maps` (positions, `road_paths`, zone, fond) ; fond PNG/JPEG/WebP (GridFS).
- Personnage qui suit le tracé des routes ; vue 3D détaillée ; vue 2D avec fond.

## 4. Contraintes de Discord (vérifiées dans la documentation officielle)

| Limite | Valeur | Source |
|--------|-------:|--------|
| Salons par serveur (catégories comprises) | 500 | code d'erreur 30013 |
| Rôles par serveur | 250 | code d'erreur 30005 |
| Salons par catégorie | 50 | type `GUILD_CATEGORY` |
| Création de serveur par un bot | **impossible** (plus de point d'accès « Create Guild ») | `resources/guild` |

L'admin doit donc **créer lui-même les serveurs** et y inviter le bot. L'import lui indique combien il en faut.

**Budget d'un serveur** avec le modèle actuel : un lieu = 1 rôle + 1 catégorie + 1 salon ; une route = 1 rôle + 1 salon, dans des catégories de routes d'au plus 50 salons. L'installation de base consomme aussi quelques rôles et salons.

Avec 150 lieux, il reste environ 95 rôles pour les routes (250 − 150 − environ 5 rôles de base). Or un réseau de 150 lieux compte généralement 1,3 à 2 routes par lieu, soit environ 200 à 300 routes. **C'est la limite des rôles qui bloque, pas celle de 150 lieux** (voir la question Q1). *Historique : la limite est passée à 60 lieux (Q9).*

## 5. Réponses aux dernières questions (19/11/2026)

| # | Sujet | Décision |
|---|-------|----------|
| Q1 | Rôles des routes | **Option (b)** : plus de rôle par route. L'accès au salon d'une route se donne par une **permission individuelle** sur ce salon, posée au départ du voyage et retirée à l'arrivée ou à l'arrêt. Le voyage du bot (`travel/logic.rs`, `manage_roles`, `travel/service.rs`) change en conséquence, pour les commandes slash comme pour l'activité. Une migration des routes existantes (rôle → permissions) est à prévoir. Budget : 60 lieux par serveur depuis Q9 (1 rôle + 1 catégorie + 1 salon chacun), routes limitées par le nombre de salons (500 par serveur, 50 par catégorie) et par les permissions (1 000 par salon, code d'erreur 30060). |
| Q2 | Objets posés sans propriétaire | Avant la validation, l'admin est **alerté** de la liste des objets qui seront **détruits**, et **doit confirmer**. |
| Q3 | Visuels sans Azgaar dans Discord | La solution la plus durable : **le rendu 2D/3D de l'activité**. Le SVG de la ligne 5 du `.map` n'est pas utilisé. |
| Q4 | Brouillard | **Un vrai effet de brouillard** (brume animée et opaque) qui empêche toute lecture de la carte hors des zones découvertes. Ni masquage, ni assombrissement, ni noircissement. |
| Q5 | Salon générique d'un lieu | `place-centrale` (fr), `central-square` (en), selon la langue du serveur. |

## 6. Points à vérifier en début de session

- Faisabilité d'Azgaar dans l'iframe de l'activité (I3) : CSP de Discord, iframe imbriquée, polices servies depuis notre domaine. Sinon, redirection vers azgaar.github.io (instructions), puis import du `.map` et rendu par l'activité (Q3).
- Limites de débit de Discord lors de la création en masse de salons et de rôles (respect de `Retry-After`, barre de progression).
- Partition des lieux entre serveurs (I4) : graphe pondéré par les routes, au plus 60 lieux (Q9) et dans le budget de salons par serveur, en minimisant les routes entre serveurs.

## 7. Résultats des vérifications (19/11/2026, session « import de carte »)

### 7.1 Azgaar dans l'iframe de l'activité (I3)
- **Non vérifiable depuis la session cloud** : ni client Discord, ni accès à docs.discord.com depuis le réseau de la session. La documentation publique (page *Networking*) confirme que toute requête de l'activité doit passer par le proxy `<client_id>.discordsays.com` (CSP `connect-src`) ; rien de public ne précise `frame-src` pour une iframe imbriquée.
- Piste technique si on l'essaie : servir une version figée d'Azgaar sous notre domaine (`/azgaar/`, même origine que l'activité, donc via le mapping `/`), polices auto-hébergées, et lire la carte **directement dans la fenêtre enfant** (même origine) plutôt que par un téléchargement, que l'iframe de Discord risque de bloquer.
- Conséquence : on livre d'abord le **repli** (instructions + lien vers azgaar.github.io, puis import du `.map`, rendu par l'activité, Q3). L'intégration d'Azgaar est une étape à part, en dernier, qui demande un essai réel dans Discord par l'utilisateur et son accord pour exécuter le code d'Azgaar.

### 7.2 Limites de débit de Discord (mesurées sur « rpbot test »)
Script : 30 salons puis 30 rôles créés à la suite, puis supprimés.

| Point d'accès | `X-RateLimit-Limit` | Fenêtre observée | Débit réel |
|---|---:|---|---|
| `POST /guilds/{id}/channels` | **2 000** | réinitialisation en minutes | ≈ 0,35 s par salon, aucun 429 |
| `POST /guilds/{id}/roles` | **250** | `Reset-After` ≈ **21 000 s (≈ 6 h)** | ≈ 0,45 s par rôle, aucun 429 |
| Suppressions | — | ne consomment pas les quotas ci-dessus | — |

Conséquences :
- **Quota de 250 créations de rôles par serveur sur plusieurs heures**, indépendant de la limite de 250 rôles présents. 60 lieux + 4 rôles de base passent (Q9), mais **un import raté puis relancé** consomme à nouveau le quota. Confirme Q1 (plus de rôle par route) et impose : (1) ne jamais recréer un rôle existant (I10), (2) **espacer les créations de rôles** et afficher l'attente imposée par Discord dans la progression (Q8 ; serenity dort déjà sur `Retry-After`).
- Durée estimée d'un serveur complet (60 lieux = 180 objets Discord + ~90 routes) : **environ 2 minutes**, d'où la barre de progression (I13).
- serenity 0.12 respecte déjà `Retry-After` et la limite globale (`http/ratelimiting.rs`).

### 7.3 Permissions individuelles sur un salon de route (Q1)
Essai `PUT /channels/{id}/permissions/{user_id}` (type membre) sur un salon temporaire :
- membre du serveur : **204** ; non-membre : **404 `Unknown Overwrite` (10009)** → même comportement que `manage_roles`, qui ignore déjà un joueur absent du serveur ;
- `DELETE` d'une permission absente : **204** (idempotent, donc sûr pour l'arrêt et l'arrivée rejoués après un redémarrage).

### 7.4 Répartition des lieux entre serveurs (I4)
Budget d'un serveur : lieux ≤ **60** (Q9) ; rôles `lieux + 4 ≤ 250` ; salons `base + 2 × lieux + routes hébergées + ⌈routes hébergées / 50⌉ ≤ 500` (base : 9 salons en installation complète, 1 en partielle). Une route entre deux serveurs est hébergée par l'un des deux (celui qui a le plus de marge) ; ses voyageurs doivent être membres des deux serveurs, comme aujourd'hui (`server_two_id`).

Algorithme retenu (pur, déterministe, testé hors Discord) :
1. `k` = nombre minimal de serveurs, d'abord `⌈lieux / 60⌉`, augmenté tant qu'aucune répartition ne respecte les budgets ;
2. graines éloignées (point le plus loin du précédent, distance par les routes), puis **croissance de régions** par Dijkstra multi-source, en respectant la capacité ;
3. **raffinement Fiduccia–Mattheyses** : déplacer un lieu de bordure si cela réduit le nombre de routes entre serveurs sans dépasser un budget ;
4. résultat : affectation lieu → serveur, nombre de routes coupées, budget restant de chaque serveur (affiché à l'admin avant validation).

## 8. Découpage proposé (à valider par l'utilisateur)

Chaque étape est testée, poussée sur `feature/map-import`, et met à jour `PROGRESS.md` et la PR.

| # | Étape | Contenu |
|---|---|---|
| E1 | Routes sans rôle (Q1) | Permission individuelle posée au départ, retirée à l'arrivée et à l'arrêt, dans `travel/logic.rs`/`service.rs` (donc identique slash et activité). `Road.role_id` optionnel ; `/road_create_road` ne crée plus de rôle ; migration des routes existantes. Test REST. |
| E2 | Services de création sans `poise::Context` | Lieu (rôle + catégorie + salon `place-centrale`/`central-square`, I5, Q5), route, installation d'univers et de serveur, partagés par les commandes slash et l'import. Exécuteur de plan avec journal, progression et **retour en arrière complet** (I13), arrêt sur quota. |
| E3 | Plan d'import (logique pure) | `.map` → lieux, routes, distances brutes (I8), routes maritimes, secrètes, rattachement (I16), différence de ré-import (I10), répartition (§ 7.4). |
| E4 | API admin | Activité sans univers (I2), droits (I1, revérifiés sur chaque serveur cible), envoi du `.map`, aperçu, choix des serveurs (I9), création de l'univers (I14), validation par le nom (I12), tâche avec progression par WebSocket. |
| E5 | Écrans de l'activité | Instructions Azgaar, aperçu 2D/3D, clic sur une route → « secrète », case « routes maritimes », nombre de serveurs, récapitulatif, barres de progression. |
| E6 | « Supprimer les lieux existants » (I15, Q2) | Lieu de départ, voyages annulés, objets posés détruits ou rendus, alerte et confirmation. |
| E7 | « Générer une map » (I11) | Graine, nombre de lieux, édition simple. |
| E8 | Brouillard (I17, Q4) | Option par univers, brume animée opaque. |
| E9 | Azgaar dans l'activité (I3) | Facultatif, après accord et essai réel dans Discord. |

## 9. Décisions du 19/11/2026 (session « import de carte »)

| # | Sujet | Décision |
|---|-------|----------|
| Q6 | Découpage | Ordre E1 → E9 validé. |
| Q7 | Anciens rôles de route | La migration **supprime** chaque rôle de route une fois les permissions individuelles posées. |
| Q8 | Quota de 250 créations de rôles | **Temporiser** la création pour l'étaler, plutôt qu'annuler. Selon l'utilisateur, atteindre 250 créations revient de toute façon à atteindre la limite dure de 250 rôles présents. Nuance retenue pour la mise en œuvre : le quota compte aussi les rôles créés puis supprimés (retour en arrière, nouvel essai). L'exécuteur espace donc les créations de rôles, ne recrée jamais un rôle existant (I10), et affiche l'attente dans la barre de progression quand Discord en impose une. |
| Q11 | Choix des serveurs | L'import est bloqué tant que tous les serveurs nécessaires ne sont pas renseignés (§ 13). |
| Q10 | Serveurs par univers | Plus de limite pour l'instant ; elle reviendra comme fonctionnalité premium (§ 11). |
| Q9 | Lieux par serveur | **60 au plus** (au lieu de 150). Raison donnée par l'utilisateur : avec 2,5 routes par lieu en moyenne, 60 lieux + 150 extrémités de routes restent sous 250. Mesuré sur Moria (`a_real_map_save_is_read`) : 656 routes terrestres pour 532 bourgs desservis, soit **2,47 routes par bourg** (2,97 avec les routes maritimes). Budget d'un serveur de 60 lieux : 64 rôles, environ 9 + 120 + 75 à 100 routes ≈ 210 à 230 salons. Le quota de 250 créations de rôles permet ainsi plusieurs essais. Moria entière (538 bourgs) demanderait 9 serveurs. |

## 10. E1 livrée : routes sans rôle (19/11/2026)

- `Road.role_id` devient optionnel ; `/road_create_road` ne crée plus de rôle, son salon est seulement fermé à @everyone (`road_channel_permissions`).
- Voyage (`travel/logic.rs`, appelé par `travel/service.rs`, donc **identique par commande slash et par l'activité**) : `grant_road_access` au départ (`add_travel`), `revoke_road_access` à l'arrivée (minuteur et reprise au démarrage). Un joueur qui rejoint le serveur de la route reçoit son accès (`guild_member_addition`).
- **Arrêt en cours de route** : comme avec l'ancien rôle, le groupe arrêté **reste sur la route** et garde l'accès à son salon ; l'accès est retiré quand il en sort (arrivée, y compris après un demi-tour). Retirer l'accès à l'arrêt laisserait un joueur sur une route dont il ne voit plus le salon.
- Migration (`roads/migration.rs`, au démarrage du bot, idempotente) : permission individuelle pour les joueurs sur la route, suppression du rôle (Q7 ; un rôle déjà supprimé compte comme un succès), retrait de `role_id`. Un rôle non supprimable est retenté au démarrage suivant.
- Tests : `a_road_role_is_migrated_to_an_individual_permission` et `a_trip_grants_then_revokes_the_road_access` (MongoDB + API REST, vrai salon, vrai membre), tests unitaires des permissions et de la liste des voyageurs.

## 11. E2 en cours : services de création (19/11/2026)

### E2a, livrée : exécuteur et services lieu et route
- `src/provisioning/` : `Provisioner` (client HTTP seulement, pas de `poise::Context`). Journal des objets créés (rôles, salons, documents) ; pour une tâche longue, journal écrit en base (`provisioning_jobs`) à chaque création. Retour en arrière dans l'ordre inverse, un objet déjà absent compte comme supprimé ; progression `watch` (phases `Creating`, `Waiting`, `RollingBack`, `Done`, `RolledBack`, `RollbackIncomplete`). Créations de rôles espacées de 1,5 s au moins (Q8).
- Au démarrage du bot, les tâches restées `Running` (bot arrêté au milieu d'un import) sont entièrement défaites (`rollback_interrupted_jobs`).
- `provision_place` : rôle, catégorie visible du seul rôle, salon générique `place-centrale` / `central-square` selon la langue du serveur (I5, Q5), synchronisé avec la catégorie ; `Place.central_channel_id` (nouveau champ optionnel). `/place_create_place` l'utilise.
- `provision_road` : salon sans rôle (Q1) dans la catégorie donnée (l'import pourra répartir les routes sur plusieurs catégories de 50), `server_two_id` si la route relie deux serveurs. `/road_create_road` l'utilise.
- Tests contre l'API REST (exécutés sur RpBotTest2) : échec d'une route après un lieu complet → **tout** est défait, lieu compris ; tâche interrompue défaite au démarrage ; progression de la création puis du retour en arrière. Les tests vérifient MongoDB avant de créer quoi que ce soit sur Discord.

### E2b, livrée : installation d'un univers et d'un serveur sans `poise::Context`
- `provisioning/server_setup.rs` : installation partielle (4 rôles de base + catégorie des routes) ou complète (+ 3 catégories et 7 salons), noms dans la langue demandée. Ce qui existe encore est réutilisé, jamais recréé ; le document `servers` n'est écrit qu'à la fin. Budget : `setup_budget` (1 ou 11 salons, 4 rôles).
- **Correction** : l'ordre des rôles demandait à Discord de déplacer le rôle du bot et les rôles au-dessus de lui, ce qui échoue (50013 « Missing Permissions », vu sur RpBotTest2 ; `/universe_setup` échouait donc déjà sur un tel serveur). Désormais seuls les 4 rôles de base sont placés, juste sous le rôle le plus haut du bot (`base_role_positions`).
- `provisioning/universe.rs` : `create_universe` (nom validé, vitesse du temps facultative de 1 à 10 000 %, temps réel par défaut, I14 ; univers, serveur et stat de vitesse dans une transaction, puis installation ; en cas d'échec l'univers et ses documents sont supprimés aussi) et `add_server`.
- `/universe_setup`, `/universe_create_universe` et `/universe_add_server` utilisent ces services. L'ancien code d'installation (`partial_setup.rs`, `complementary_setup.rs`, `full_setup.rs`, `Server::rollback`, `Server::snaphot`, `IdExt`) est supprimé.
- Tests contre Discord, sur les deux serveurs de test : univers avec installation complète, rôles dans le bon ordre, puis retour en arrière complet (Discord et base) ; installation partielle puis complète sans recréer de rôle ; paramètres invalides sans aucun appel à Discord.
- CI : les tests Discord portent `discord` dans leur nom et les tests sur une vraie carte commencent par `a_real_` ; le job d'intégration les saute (`--skip discord --skip a_real_`). Les tests Moria faisaient échouer ce job sur la PR #12.

### Limite de serveurs par univers (Q10)
Elle était de 2 (`FREE_LIMIT_SERVERS_PER_UNIVERSE`), ce qui plafonnait l'import à 120 lieux. **Décision : supprimée pour l'instant** (`MAX_SERVERS_PER_UNIVERSE = None`), pour l'import comme pour `/universe_add_server`. **À réintroduire plus tard comme fonctionnalité premium** (dépôt privé). La limite de 2 univers par créateur est inchangée.

## 12. E3 livrée : plan d'import (19/11/2026)

Logique pure, sans Discord ni base : `activity/map/partition.rs` et `activity/map/import_plan.rs`.

- **Répartition** (§ 7.4) : `k` minimal, graines éloignées, croissance de régions par Dijkstra sur les routes, raffinement des lieux de bordure. Lieux déjà créés **fixés** sur leur serveur ; lieux et routes existants comptés dans la limite de lieux mais pas deux fois dans le budget de salons et de rôles. Une route entre deux serveurs est hébergée par celui qui a le plus de salons libres.
- **Plan** (`build_plan`) : bourgs retenus (sélection, zone), routes candidates (maritimes si demandées), secrets choisis dans l'aperçu, distances arrondies au km avec 1 km au moins. Trois modes :
  - `Replace` (I15) : tout l'existant est listé pour suppression ;
  - `Merge` (I16) : les rattachements choisis par l'admin (proposés par `match_places`) gardent le lieu et son nom ; rien n'est supprimé ;
  - `Reimport` (I10) : les lieux d'un import précédent (bourg connu) sont gardés avec leur nom ; les routes changées sont mises à jour ; ce qui a disparu est retiré. Les routes manuelles vers des lieux manuels ne sont pas touchées.
- Le plan donne : actions par lieu et par route, serveur de chaque lieu, serveur hôte de chaque route, budget de chaque serveur, **nombre de serveurs à créer** par l'admin (I4), routes entre serveurs, bourgs isolés, et le nombre d'objets Discord à créer (total de la barre de progression).
- **Moria entière** (`a_real_map_is_planned_over_several_servers`) : 538 lieux, 656 routes, **9 serveurs** (le minimum), 88 routes entre serveurs (13,4 %), 6 bourgs isolés, 2 288 objets Discord, calcul en 0,17 s. Par serveur : 59 ou 60 lieux, 197 à 212 salons, 63 ou 64 rôles.

### Question ouverte : unité des distances
`road_candidates` convertit en km les cartes en miles, lieues ou milles marins (`unit_to_km`). I8 dit « aucune conversion ». Moria est en km, donc sans effet ici. À trancher : garder la conversion (cohérente avec les vitesses en km/h) ou garder le nombre de la carte tel quel.

## 13. E4 en cours : API admin (19/11/2026)

### E4a, livrée : activité sans univers, droits, envoi de la carte, aperçu
- **Activité sans univers (I2)** : `Session.universe_id` est optionnel. Sur un serveur relié à aucun univers, seul un administrateur (propriétaire, permission ADMINISTRATOR) obtient une session ; un joueur reçoit `activity__server_not_linked`. Les routes de jeu répondent alors `activity__server_not_linked`. La réponse d'authentification porte `universe: null` et `can_import`. Le front affiche pour l'instant un message d'attente (écrans en E5).
- **Droits (I1)** : `may_import` = administrateur ou créateur de l'univers, **recalculé à chaque requête** à partir de Discord et de la base (`require_importer`). Chaque serveur cible est vérifié (`check_target`) : l'admin y est administrateur, le bot y est, et le serveur n'est relié à aucun univers ou au même ; sinon 403 ou 409.
- Routes (`activity/admin_import.rs`) :
  - `GET /api/admin/import/servers` : serveurs du bot (50 au plus) où l'admin est membre, avec `eligible`, `linked` (`none`, `this`, `other`) et `current` ;
  - `POST /api/admin/import/map` : sauvegarde `.map` brute, **32 Mio au plus** (les autres routes gardent 64 Kio), lue hors du fil asynchrone ; renvoie bourgs, terrain, unités, et les rattachements proposés par nom (I16). La carte est gardée 1 h en mémoire, 16 cartes au plus ;
  - `POST /api/admin/import/plan` : choix de l'admin (mode, routes maritimes, sélection de bourgs, secrets, rattachements, serveurs, type d'installation, zone), stricts et bornés ; renvoie le plan, le nombre de serveurs à créer et le nombre d'objets Discord.
- Débit : 1 action/s soutenue par admin sur l'envoi et l'aperçu.
- Tests (routeur complet, faux Discord, MongoDB) : un admin d'un serveur sans univers envoie une carte et obtient un plan ; un joueur est refusé ; un serveur d'un autre univers (409) ou dont l'admin n'est pas membre (403) est refusé même si le client l'envoie ; l'activité s'ouvre sans univers pour un admin seulement ; seule la route de la carte accepte un gros corps.

### E4b, livrée : exécution
- `POST /api/admin/import/execute` (`activity/import_exec.rs`) : `{plan, confirm_name, new_universe?}`. Le plan est **recalculé côté serveur**. Double vérification (I12) : `confirm_name` doit être exactement le nom de l'univers, ou celui de l'univers à créer (I14 : nom, vitesse du temps facultative). Refus 409 si des serveurs manquent (`activity__import_servers_missing`), si le plan supprime de l'existant (`activity__import_removal_not_ready`, en attendant E6), ou si un import tourne déjà sur ce serveur.
- Tâche de fond, journal en base : univers et premier serveur si besoin, autres serveurs rattachés et installés, catégories de routes supplémentaires (50 salons chacune, nommées « Routes 2 », « Routes 3 »…), lieux, routes, puis **une transaction** pour les mises à jour des routes existantes et la carte `universe_maps` (positions avec le bourg d'origine pour un ré-import, tracés, relief ramené de 0–100 à 0–255, zone). Au moindre échec, retour en arrière complet.
- `GET /api/admin/import/jobs/{id}` : progression (`phase`, `done`, `total`) et issue (`running`, `done`, `failed` avec la clé d'erreur). Une tâche n'est visible que depuis son serveur.
- À la fin, les sessions du serveur sans univers sont reliées au nouvel univers sans reconnexion (`SessionStore::link_universe`), et la carte envoyée est oubliée.
- Limites connues : pas d'annonce dans le forum wiki pour les lieux importés (des centaines de messages) ; les catégories de routes supplémentaires ne sont pas enregistrées sur le serveur, donc `/road_create_road` continue d'utiliser la première.
- Tests réels sur RpBotTest2 (et « rpbot test ») : import d'une petite carte dans un nouvel univers à travers le routeur (mauvais nom refusé sans rien créer ; univers, lieux avec salon générique, route, carte et session reliée vérifiés ; tout nettoyé ensuite) ; échec simulé après le premier lieu → plus rien sur Discord ni en base.

### Choix des serveurs (Q11, 19/11/2026)
Décision : **l'import reste bloqué tant que tous les serveurs nécessaires ne sont pas renseignés.** Mise en œuvre :
- Une activité ne peut pas lister les serveurs de l'utilisateur sans le scope OAuth `guilds`, et cette liste viendrait du client. La liste est donc faite **par le bot** (`GET /api/admin/import/servers` : serveurs du bot où l'admin est administrateur, vérifiés côté serveur), qui renvoie aussi le **lien d'invitation du bot** (`invite_url`, permission Administrateur, commandes).
- L'écran (E5) montre un emplacement par serveur nécessaire : le serveur courant, puis un menu par emplacement. Un bouton ouvre le lien d'invitation (`openExternalLink` du SDK, Discord demande confirmation) ; l'admin crée le serveur dans Discord puis y invite le bot.
- **Mise à jour sans interroger Discord en boucle** : quand le bot rejoint ou quitte un serveur (`guild_create`, `guild_delete` ; intent `GUILDS` ajouté), l'évènement `servers_updated` est poussé par WebSocket aux administrateurs, et l'écran relit la liste.
- « Valider » reste grisé tant qu'un emplacement est vide ; le serveur refuse aussi (`activity__import_servers_missing`).

## 14. E5 livrée : écrans de l'activité (19/11/2026)

- `web/activity/src/admin/` : `ImportWizard` (chargé à la demande), `ImportPreview`, `importModel` (logique testée).
- **Serveur sans univers** : l'admin arrive directement sur l'assistant ; un joueur voit un message d'attente. **Univers existant** : en mode admin, bouton « Carte de l'univers » (créateur ou administrateur seulement).
- Étapes : choix « Importer une carte » / « Générer une map » (E7, grisé) → instructions Azgaar (lien vers azgaar.github.io par `openExternalLink`, « Save » puis « Save to machine ») et choix du `.map` → aperçu et réglages → récapitulatif et saisie du nom (I12) → progression (création puis, en cas d'erreur, retour en arrière en rouge) → « Ouvrir l'univers » (nouvelle connexion silencieuse).
- **Aperçu** (Q3, rendu de l'activité) : relief Azgaar lissé, lieux colorés par serveur, routes (jaune : entre deux serveurs ; tirets : maritimes ; rouge en tirets : secrètes). **Clic sur une route : secrète ou publique** (I7). Molette : zoom ; glisser : déplacer.
- **Réglages** : mode (si un univers existe : créer en plus, ré-import, remplacer), routes maritimes (décochées), type d'installation des nouveaux serveurs, rattachements proposés par nom (créer en plus).
- **Serveurs (Q11)** : un emplacement par serveur nécessaire avec son budget (lieux, salons) ; menus limités aux serveurs utilisables et pas déjà pris ; bouton d'invitation du bot ; liste relue à l'évènement `servers_updated`. « Suivant » et « Créer » restent grisés tant qu'un emplacement est vide (et le serveur refuse aussi).
- Nouvel univers : nom (prérempli avec celui de la carte) et vitesse du temps facultative (I14).
- Aperçu recalculé 1,2 s après le dernier changement (le serveur limite à 1 par seconde).
- **Vérifié dans un vrai navigateur** (Chromium, Playwright) avec Moria : page `web/activity/import-harness.html` (développement seulement, absente du build) et scénario `web/activity/e2e/import-harness.cjs`, sur des réponses calculées par le code Rust (`a_real_map_preview_fixture_is_written`). Bureau et téléphone ; aucune erreur dans la console.
- Limites : pas encore de zoom à deux doigts sur l'aperçu (le glisser marche) ; pas de sélection manuelle des bourgs ni de zone explorable dans l'écran (l'API les accepte déjà) ; l'estimation de durée ne compte pas l'installation des nouveaux serveurs.

## 15. E6 livrée : « Supprimer les lieux existants » et retraits du ré-import (19/11/2026)

- `activity/import_removal.rs`. S'applique au mode remplacer (I15) et aux lieux et routes disparus d'un ré-import (I10).
- **Aperçu** : `POST /api/admin/import/plan` renvoie `removal` : joueurs à déplacer (dont en voyage) et objets posés dans les lieux supprimés (nom, lieu, propriétaire ou aucun, nombre d'objets dedans).
- **Choix** (`removal` dans `POST /api/admin/import/execute`) : `start_burg` (lieu de départ, un bourg de la nouvelle carte ; obligatoire s'il y a des joueurs à déplacer), action par objet posé (`return` par défaut s'il a un propriétaire, ou `destroy`), `confirm_destroy_unowned` (Q2 : obligatoire s'il y a des objets sans propriétaire ; un objet sans propriétaire ne peut pas être rendu). Vérifiés côté serveur.
- **Déroulé** : tout le nouveau est créé (et peut être défait). Les voyages concernés sortent de la file en mémoire. Puis **une transaction** : carte, mises à jour des routes, groupes déplacés au lieu de départ (voyages annulés), objets rendus (l'objet et son contenu vont dans l'inventaire du propriétaire) ou détruits (contenu compris), suppression des lieux, routes, zones (`areas`) et routes connues. C'est le point de non-retour : en cas d'échec de la transaction, les voyages reviennent dans la file et la création est défaite. Ensuite, sur Discord : rôle du lieu de départ aux joueurs déplacés, suppression des salons des routes, des salons et catégories des lieux, et de leurs rôles (un objet déjà absent compte comme supprimé ; un échec est journalisé).
- Groupes touchés : tous en mode remplacer ; en ré-import, ceux qui sont dans un lieu supprimé, sur une route supprimée, ou en route depuis ou vers un lieu supprimé.
- **Écran** : section « Lieux existants supprimés » (comptes, lieu de départ, action par objet, liste des objets sans propriétaire et case de confirmation) ; « Suivant » et « Créer » grisés tant que ce n'est pas rempli ; avertissement au récapitulatif (cette partie ne peut pas être annulée). Vérifié dans Chromium.
- Test réel sur RpBotTest2 et « rpbot test » : premier import, puis un joueur, un objet posé avec contenu et un objet sans propriétaire ; le second import en remplaçant est refusé sans confirmation, puis passe : anciens salons, catégories et rôles supprimés de Discord, joueur au lieu de départ avec son rôle, objet et contenu rendus, objet sans propriétaire détruit.

## 16. E7 livrée : « Générer une map » (19/11/2026)

- **Générateur** (`activity/map/generate.rs`), en Rust, **déterministe** : générateur pseudo-aléatoire maison (SplitMix64), indépendant des versions de bibliothèques ; une valeur figée dans les tests garantit qu'une graine déjà montrée redonne la même carte. Carte de 1 000 × 1 000 unités ; lieux espacés (échantillonnage par rejet) ; **routes : graphe de voisinage relatif** (connexe, sans croisement ; 2,5 à 2,9 routes par lieu) ; tracés en courbes de Bézier quadratiques ; voisins à environ 30 km ; noms par syllabes sans doublon ; capitale au centre. 600 lieux : 71 ms (11 ms en version optimisée).
- **API** : `POST /api/admin/import/generate` `{seed?, count}` (graine sur 32 bits, exacte en JavaScript ; 2 à 600 lieux) ; `POST /api/admin/import/generated` : la carte modifiée, **vérifiée côté serveur** (noms, positions, lieux connus, pas de doublon ni de boucle), gardée comme une carte envoyée. La suite (aperçu, serveurs, confirmation, création, suppression) est celle de l'import, avec **exactement les routes dessinées** (aucun détour retiré). La carte est enregistrée comme `Manual`, **sans relief** : l'activité affiche son terrain 3D procédural (I11).
- **Éditeur** (`web/activity/src/admin/GenerateEditor.tsx`, logique dans `generateModel.ts`) : nombre de lieux, graine affichée, « Générer » et « Nouvelle graine » ; outils : sélectionner et renommer, ajouter un lieu (clic), ajouter une route (deux lieux), supprimer un lieu (et ses routes) ou une route, retoucher un tracé (poignée au milieu de la courbe, même formule que le serveur) ; bascule **vue 3D** (la scène détaillée de l'activité). Pas de modification en profondeur (relief, biomes).
- Vérifié dans Chromium (`web/activity/e2e/generate-harness.cjs`) : renommage, ajout d'un lieu et d'une route, suppression d'une route, retouche d'un tracé, vue 3D, passage à l'aperçu. Test réel sur Discord : une carte générée de 3 lieux est créée (lieux, routes, carte `Manual` sans relief), puis nettoyée.

## 17. E8 livrée : brouillard (19/11/2026)

- **Option par univers** (I17) : `Universe.fog_enabled` (désactivée pour les univers existants). Choisie à la création par l'import ou la génération (case « Brouillard »), puis activée ou coupée par **le créateur seulement** (`POST /api/admin/universe/fog`, vérifié en base ; paramètres de l'activité, section « Univers »). Les cartes ouvertes se rechargent.
- **Protection côté serveur** (`activity/fog.rs`) : sous brouillard, un **joueur** ne reçoit que les lieux découverts et ses destinations possibles, les routes empruntées, la route en cours et celles qui partent d'ici ; le relief est effacé hors des zones découvertes ; l'**image de fond Azgaar n'est jamais servie** (elle dévoilerait la carte). Les modérateurs et les administrateurs voient toute la carte. La réponse porte `fog` : rayon, lieux découverts, destinations entrevues, routes découvertes.
- Zones découvertes : la position, le départ et l'arrivée du trajet en cours, les extrémités des routes déjà empruntées (`known_roads`), et un couloir le long de ces routes. Rayon : 60 % de la longueur médiane des routes.
- **Rendu** (Q4 : une vraie brume, ni masque ni assombrissement) :
  - 3D : plan au-dessus du relief, shader de bruit fractal animé (30 images par seconde, seulement quand le brouillard est actif), opaque ; un masque dessiné dans un canevas dégage les zones découvertes, au bord ondulant ;
  - 2D : brume SVG opaque (base et bruit fractal qui dérive et respire, piloté en JavaScript à 15 images par seconde), trouée par un masque aux bords adoucis ;
  - **au-dessus de la brume** (retour utilisateur) : les routes envoyées (empruntées et celles qui partent d'ici, jusqu'aux destinations entrevues), les points des lieux envoyés et leurs noms, en 3D comme en 2D. Seul ce que le serveur envoie est dessiné, donc rien n'est dévoilé de plus ; la brume est aussi plus basse en 3D (juste au-dessus du niveau de la mer) pour que les points restent visibles.
  - **Grandes cartes Azgaar** (vérifié sur Moria, 436 × 913, rayon de zone ≈ 6) : la vue de départ cadre la zone découverte (`fogFocus` : tout ce qui est dégagé, marge de deux rayons, au moins huit rayons de côté) au lieu de toute la carte, où elle n'était qu'un point ; le masque 3D ne couvre que cette zone (1024 px), sinon le trou ne faisait que quelques pixels ; le grain de la brume suit la taille de la zone.
  - 3D : le relief est **aplani sous la brume** (`flattenUnderFog`, fondu sur une demi-zone) et la brume est posée au ras de la mer : pas de décalage de parallaxe entre le trou de la brume et les lieux, et les collines découvertes émergent de la brume. Cela couvre aussi le relief généré par le client pour les cartes sans heightmap.
  - 3D, avec ou sans brouillard : la taille des pastilles et des routes suit l'écart entre les lieux (`markerScale`, de 0,2 à 1) ; sur Moria, des pastilles de taille fixe se chevauchaient.
- Fixture réelle : `a_real_map_fog_fixture_is_written` (`import_exec.rs`, ignoré, `AZGAAR_MAP_FILE` et `IMPORT_FIXTURE_DIR`) écrit la carte Moria qu'un joueur reçoit sous brouillard après deux routes, pour `fog-harness.html`.
- Vérifié dans Chromium (`fog-harness.html`, page de développement absente du build ; `web/activity/e2e/fog-harness.cjs`) : brume opaque hors des zones découvertes, animée en 3D et en 2D. Test de bout en bout contre MongoDB : un joueur ne reçoit ni lieu lointain ni image de fond, un modérateur voit tout, un joueur non créateur ne peut pas couper le brouillard, le créateur le coupe et la carte redevient entière.
