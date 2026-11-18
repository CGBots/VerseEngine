# Activité Discord — suivi entre les sessions

Dernière mise à jour : 18/11/2026 (session « environnement de test Discord »). Ce fichier, `ARCHITECTURE.md` et `DEPLOY.md` (même dossier) sont la référence pour reprendre le travail.

## État des branches

| Branche | PR | État |
|---|---|---|
| `chore/hardening` | [#11](https://github.com/CGBots/VerseEngine/pull/11) | **Prête à fusionner.** CI verte (tests, intégration MongoDB, image Docker). Validée contre l'API REST Discord. |
| `feature/discord-activity` | [#12](https://github.com/CGBots/VerseEngine/pull/12) (brouillon) | Contient #11 par fusion. Phase b terminée côté code ; reste un essai réel dans Discord, qui demande l'hébergement. |

- `cargo test --locked` sur `feature/discord-activity` : 184 réussis, 17 ignorés (intégration).
- `cargo test --locked -- --ignored --skip test_discord_bot_connection` : 16 réussis contre MongoDB 8.0 (replica set) et l'API REST Discord.
- `npm test` (web/activity) : 9 réussis ; `npm run build` OK.
- Reprise : `add_repo` (accès push), puis `git checkout feature/discord-activity`. Pas de force-push, pas de push sur `main`. Les PR passent par l'API REST : GraphQL n'est pas disponible.

## Environnement de test (session cloud)

- Le proxy injecte le jeton du bot de **test** sur toutes les requêtes `discord.com`, et remplace tout en-tête `Authorization` déjà présent, `Bot` comme `Bearer`. Conséquence : le flux OAuth utilisateur (`/users/@me` avec un jeton utilisateur) **ne peut pas** être testé ici, il renverrait le bot. La gateway n'est pas joignable non plus.
- Serveur de test : « rpbot test » (`1080809463201677332`). Les tests REST créent puis suppriment leurs propres salons.
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

## Points ouverts

- **Hébergement** : on ne sait pas encore si l'infra a déjà un reverse proxy. `DEPLOY.md` décrit Caddy et les exigences pour un autre proxy.
- Essai réel dans Discord (auth SDK, iframe, CSP, WebSocket via le proxy) : impossible depuis la session cloud.
- Commandes slash : elles répondent maintenant à l'interaction par un message éphémère (avant, elles ne répondaient pas). Une destination invalide n'arrête plus un groupe en route. Le menu de `/travel_start` montre les routes secrètes déjà empruntées (D10).
- Messages publics en fin de trajet (`travel/logic.rs`) toujours en `fr` en dur (D12 : langue de la guilde proposée, non faite).
- Annexe d'`ARCHITECTURE.md` § 2 : ré-accepter un personnage existant échoue (`_id` modifié par `replace_one`). Non corrigé.
- Étiquettes de lieux dans la scène 3D : absentes (noms visibles dans le panneau et la vue 2D).

## Prochaine étape (phase c : inventaire)
1. `Inventory.equipped: bool` (`#[serde(default)]`), sans emplacement (D6).
2. Extraire `item use` / `item consume` dans un service partagé, comme pour le voyage.
3. `GET /api/me/inventory`, `POST /api/me/inventory/{id}/{use,consume,equip,unequip}` ; évènement `inventory_updated` (holder → joueur).
4. Panneau d'inventaire dans le front.
