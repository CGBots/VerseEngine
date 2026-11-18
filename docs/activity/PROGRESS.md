# Activité Discord — suivi entre les sessions

Dernière mise à jour : 18/11/2026. Ce fichier et `ARCHITECTURE.md` (même dossier) sont la référence pour reprendre le travail.

## État
- Branche `feature/discord-activity` poussée : PR brouillon #12 (https://github.com/CGBots/VerseEngine/pull/12).
- Elle contient, par fusion, `chore/hardening` (PR #11 : Docker, CI, Cargo.lock, unwraps, tests, cycle jour/nuit). Conflit résolu : `calculate_distance_at` (`src/travel/logic.rs`) est la seule fonction pure de distance ; `src/activity/travel_view.rs` l'utilise.
- `cargo test --locked` sur la branche : 129 réussis, 9 ignorés (intégration MongoDB/Discord, `cargo test -- --ignored`).
- Reprise : `add_repo` (accès push), cloner, `git checkout feature/discord-activity`. Pas de force-push, pas de push sur `main` ; PR via l'API REST (`gh api repos/CGBots/VerseEngine/pulls/...`), GraphQL indisponible.

## Décisions utilisateur (D1–D12)
Toutes reçues le 18/11/2026, détaillées dans `ARCHITECTURE.md` § 13. Résumé : même conteneur ; app du bot ; modérateurs = joueurs + propositions, pas d'édition d'univers ; fiches en base ; image en paramètre de commande ou via fil, supprimée avec le message ; pas d'emplacement d'équipement ; zone explorable à l'import ; distances Azgaar sommées aux bifurcations ; logs de tout le gameplay ; routes secrètes visibles si déjà empruntées ; Cargo.lock versionné ; langue du joueur.

## Fait
- Phase a complète + logique pure phase b (layout, Azgaar, réseau routier, zone explorable, ETA).
- `Character.description`, `Character.image` (référence de pièce jointe), `Cargo.lock`.

## Points ouverts hors activité
- Les `setup()` du voyage, du craft et du loot remplacent leur minuteur sans arrêter l'ancien à chaque reconnexion (même défaut que l'ancien cycle jour/nuit). Non corrigé.
- PR #11 jamais testée contre un vrai Discord.

## Prochaine étape (phase b)
1. `src/travel/service.rs` : extraire start/stop/join/leave (sans `poise::Context`), rebrancher les commandes slash ; y ajouter l'alimentation de `known_roads`.
2. `/api/map` (routes secrètes filtrées par `known_roads`), `/api/me/travel*`, `universe_maps`.
3. Hub change streams → WebSocket ; résolution de `Character.image` en URL.
4. Scène three.js / r3f ; Dockerfile multi-étapes (D1).
