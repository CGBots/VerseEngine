# Activité Discord — suivi entre les sessions

Dernière mise à jour : 19/11/2026 (session « import de carte », branche `feature/map-import`). Ce fichier, `ARCHITECTURE.md` et `DEPLOY.md` (même dossier) sont la référence pour reprendre le travail.

## État des branches

| Branche | PR | État |
|---|---|---|
| `chore/hardening` | [#11](https://github.com/CGBots/VerseEngine/pull/11) | **Prête à fusionner.** CI verte (tests, intégration MongoDB, image Docker). Validée contre l'API REST Discord. |
| `feature/map-import` | [#13](https://github.com/CGBots/VerseEngine/pull/13) (brouillon, vers `feature/discord-activity`) | Import et génération de carte : E1 à E8 livrées. |
| `feature/discord-activity` | [#12](https://github.com/CGBots/VerseEngine/pull/12) (brouillon) | Contient #11 par fusion. Phase b terminée côté code ; reste un essai réel dans Discord, qui demande l'hébergement. |

- `cargo test --locked` sur `feature/discord-activity` : 194 réussis, 18 ignorés (intégration).
- `cargo test --locked -- --ignored --skip test_discord_bot_connection` : 17 réussis contre MongoDB 8.0 (replica set) et l'API REST Discord.
- `npm test` (web/activity) : 24 réussis ; `npm run build` OK.
- Reprise : `add_repo` (accès push), puis `git checkout feature/discord-activity`. Pas de force-push, pas de push sur `main`. Les PR passent par l'API REST : GraphQL n'est pas disponible.

## Environnement de test (session cloud)

- Le proxy injecte le jeton du bot de **test** sur toutes les requêtes `discord.com`, et remplace tout en-tête `Authorization` déjà présent, `Bot` comme `Bearer`. Conséquence : le flux OAuth utilisateur (`/users/@me` avec un jeton utilisateur) **ne peut pas** être testé ici, il renverrait le bot. La gateway n'est pas joignable non plus.
- Serveurs de test : « rpbot test » (`1080809463201677332`) et « RpBotTest2 » (`1100433828931194931`, visible par l'utilisateur avec son compte principal ; utile pour les essais multi-serveurs). Les tests REST créent puis suppriment leurs propres salons. Le salon `test-1-test-2` de RpBotTest2 est antérieur à ces tests : ne pas le supprimer.
- MongoDB local en replica set : binaires 8.0.4 (fastdl.mongodb.org) dans `/opt/mongodb`, `--replSet rs0 --auth --keyFile`. À réinstaller à chaque nouvelle session (le conteneur est éphémère).
- Variables des tests d'intégration : `MONGODB_*`, `DISCORD_TOKEN=dummy` (le proxy remplace la valeur), `VERSEENGINE_TEST_GUILD_ID`. serenity n'accepte que les racines webpki : les tests ajoutent le CA de `SSL_CERT_FILE` (`test_support::discord_http`).
- Les conteneurs Docker n'ont pas de DNS ici : l'image se construit en CI, pas en local.

## Décisions utilisateur (D1–D12)
Reçues le 18/11/2026, détaillées dans `ARCHITECTURE.md` § 13. Licence envisagée : MPL-2.0 pour le core ; les fonctions payantes iront dans un dépôt privé séparé, rien de « premium » ici. PNJ IA : en dernier, avec des modèles locaux, hors périmètre.

## Fait cette session

PR #11 (`chore/hardening`) :
- Test REST du cycle d'annonce jour/nuit : poster, modifier, retrouver dans l'historique, message supprimé (404), message non modifiable (403). Un 404 n'est plus envoyé à la suppression.
- Tests d'intégration fiabilisés : runtime tokio partagé (le client MongoDB global mourait après le premier test), anciens tests réécrits, job CI MongoDB en replica set.
- Minuteurs du voyage, du craft et du loot (`utility::timer`) : génération + `abort`, traitement détaché qu'un `abort` ne coupe pas, verrou pris par `setup()`. Corrige : voyage traité avant son échéance, objets de craft/loot donnés deux fois, rôles interrompus en fin de trajet.

PR #12 (`feature/discord-activity`) :
- `travel/service.rs` partagé par les commandes slash et l'API ; `known_roads` (D10).
- `GET /api/map` : placement calculé et mis en cache dans `universe_maps` ; routes secrètes inconnues et lieux qu'elles seules desservent filtrés côté serveur.
- `GET /api/me/travel`, `POST /api/me/travel/{start,stop,join,leave}` : 1 action/s, une action à la fois par joueur, seules les clés Fluent des erreurs sont exposées.
- Change streams → WebSocket, filtrés par droits ; `Character.image` → URL du CDN (testé contre Discord).
- Scène three.js/r3f avec position interpolée et compte à rebours ; repli 2D SVG ; Dockerfile multi-étapes ; CI du front.
- Commande Entry Point « Launch » conservée lors de l'enregistrement des commandes.
- `DEPLOY.md` : URL mapping, portail OAuth2, Caddy.

## Retours utilisateur du 19/11/2026 (faits)
- Barre du haut : heure de l'univers à droite (même calcul que `/universe_time`, fonction `universe::time::time_of_day` partagée), bouton admin à icône qui s'active **et** se désactive, roue des paramètres. « Gérer mon abonnement » est déplacé dans les paramètres.
- Carte Azgaar : **l'image exportée par Azgaar est affichée telle quelle** (vue 2D, zoom et déplacement), pour avoir exactement ses visuels. Stockage GridFS (`map_backgrounds`), PNG/JPEG/WebP seulement (vérifiés par octets magiques, SVG refusé car il peut contenir du script), route `GET /api/map/background` (CSP `sandbox`). L'envoi de l'image par l'admin reste à faire avec l'import Azgaar (phase d).
- Sans export : vue 3D plus détaillée (terrain généré, déterministe, biomes, côtes irrégulières, routes posées sur le relief).
- Lieux en cercles avec leur nom ; joueur représenté par une épingle « goutte inversée » avec sa photo de profil Discord (sinon le portrait du personnage, sinon ses initiales).

## Import Azgaar : le fichier `.map` (correction du 19/11/2026)
- L'import se fait à partir de la **sauvegarde `.map`** d'Azgaar (pas d'un PNG). Lecteur `activity/map/azgaar_file.rs` : gzip, ancien base64, lignes 0 (dimensions), 1 (unités, nom), 5 (SVG), 6–7 (relief), 15 (bourgs), 37 (routes). Format vérifié dans le code d'Azgaar 1.158 (`services/io/save.ts`, `load.ts`).
- Le personnage **suit le tracé réel de la route** (polyligne Azgaar conservée par `road_candidates`, stockée dans `universe_maps.road_paths`, envoyée avec chaque route) ; sans tracé, ligne droite.
- **À décider (visuels)** : la ligne 5 du `.map` contient le dessin SVG de la carte, mais Azgaar ne la lit plus et annonce sa suppression ; ses polices web ne se chargent pas sous la CSP Discord. Le fond PNG/JPEG/WebP déjà en place reste disponible.

## Import et génération de carte (branche `feature/map-import`, en cours)
- Vérifications du § 6 faites, résultats dans `IMPORT_SPEC.md` § 7 : quota de 250 créations de rôles par serveur (≈ 6 h), 2 000 pour les salons, permission individuelle testée (204 ; non-membre 10009 ; retrait idempotent), algorithme de répartition retenu. Azgaar dans l'iframe : non vérifiable depuis le cloud, on livre d'abord le repli.
- Découpage E1–E9 validé (`IMPORT_SPEC.md` § 8, décisions Q6–Q8 au § 9). **E1 livrée** (routes sans rôle, migration, `IMPORT_SPEC.md` § 10). **E2 livrée** (exécuteur de création avec journal et retour en arrière, services lieu, route, univers et installation de serveur, partagés avec les commandes slash, `IMPORT_SPEC.md` § 11). **E3 livrée** (plan d'import et répartition, `IMPORT_SPEC.md` § 12 ; Moria : 9 serveurs, 13,4 % de routes entre serveurs). **E4 livrée** (activité sans univers, droits recalculés, envoi du `.map`, aperçu, exécution en tâche de fond avec progression et retour en arrière, `IMPORT_SPEC.md` § 13). **E5 livrée** (écrans de l'import, vérifiés dans un navigateur avec Moria, `IMPORT_SPEC.md` § 14). **E6 livrée** (suppression des lieux existants : joueurs déplacés, objets rendus ou détruits, Discord nettoyé, `IMPORT_SPEC.md` § 15). **E7 livrée** (générateur déterministe, éditeur 2D avec vue 3D, `IMPORT_SPEC.md` § 16). **E8 livrée** (brouillard par univers, filtré côté serveur, brume animée en 3D et en 2D ; routes, points et noms des lieux envoyés au-dessus de la brume, cadrage et relief adaptés aux grandes cartes Azgaar, `IMPORT_SPEC.md` § 17). Reste **E9** (Azgaar dans l'activité, facultatif, à décider). Limite de serveurs par univers supprimée (Q10), **à réintroduire plus tard comme fonctionnalité premium**.
- Fichiers Moria redéposés : les deux tests réels passent. 60 lieux au plus par serveur (Q9).
- `cargo test --locked` : 269 réussis ; intégration (`--ignored --skip test_discord_bot_connection --test-threads=1`, avec `AZGAAR_MAP_FILE` et `AZGAAR_FULL_JSON`) : 39 réussis, sur les deux serveurs de test. `--test-threads=1` : les tests d'installation utilisent le même serveur Discord ; `npm test` : 72 réussis.

## Cahier des charges de l'import
Cahier des charges complet et validé : **`docs/activity/IMPORT_SPEC.md`** (décisions I1–I17, Q1–Q5). À traiter dans une session dédiée. Points marquants : répartition de l'univers sur plusieurs serveurs (150 lieux au plus par serveur, création des serveurs par l'admin), plus de rôle par route (permissions individuelles), brouillard optionnel par univers, double vérification par le nom de l'univers, retour en arrière complet en cas d'erreur.

## Points ouverts
- **Premium (plus tard, dépôt privé)** : réintroduire une limite de serveurs par univers (`MAX_SERVERS_PER_UNIVERSE`, `database/universe.rs`), supprimée le 19/11/2026 (Q10).
- Déploiement : option 2 choisie (redirection d'un port vers le bot, HTTPS servi par le bot). À valider d'abord : Discord accepte-t-il un port dans le mapping (`activity.home.knifetail.eu:8484` s'enregistre ; le chargement réel n'est pas encore testé). Puis certificat par validation DNS (hébergeur DNS à connaître) et HTTPS dans le bot.
- Mise en page téléphone : la fiche prend le haut de l'écran et le panneau de voyage couvre la carte. À revoir (onglets ou panneau à tirer, choix à faire).

- **Hébergement** : on ne sait pas encore si l'infra a déjà un reverse proxy. `DEPLOY.md` décrit Caddy et les exigences pour un autre proxy.
- Essai réel dans Discord (auth SDK, iframe, CSP, WebSocket via le proxy) : impossible depuis la session cloud.
- Commandes slash : elles répondent maintenant à l'interaction par un message éphémère (avant, elles ne répondaient pas). Une destination invalide n'arrête plus un groupe en route. Le menu de `/travel_start` montre les routes secrètes déjà empruntées (D10).
- Messages publics en fin de trajet (`travel/logic.rs`) toujours en `fr` en dur (D12 : langue de la guilde proposée, non faite).
- Annexe d'`ARCHITECTURE.md` § 2 : ré-accepter un personnage existant échoue (`_id` modifié par `replace_one`). Non corrigé.

## Prochaine étape (phase c : inventaire)
1. `Inventory.equipped: bool` (`#[serde(default)]`), sans emplacement (D6).
2. Extraire `item use` / `item consume` dans un service partagé, comme pour le voyage.
3. `GET /api/me/inventory`, `POST /api/me/inventory/{id}/{use,consume,equip,unequip}` ; évènement `inventory_updated` (holder → joueur).
4. Panneau d'inventaire dans le front.
