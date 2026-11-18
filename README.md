# VerseEngine

Moteur de jeu de rôle textuel pour Discord, écrit en Rust (poise / serenity) avec MongoDB.
Il ajoute à un serveur de RP des mécaniques d'immersion : univers et temps (cycle jour/nuit),
lieux et routes, déplacements avec vitesse et groupes, personnages, objets et effets,
loot, recettes et artisanat. Interface en français et en anglais.

## Démarrage rapide (Docker)

```bash
cp .env.example .env      # puis renseigner DISCORD_TOKEN, MONGODB_USER et MONGODB_PASSWORD
docker compose up -d --build
```

Le `docker-compose.yml` lance le bot et un MongoDB en replica set à un seul nœud.
Aucun secret n'est copié dans l'image : tout passe par les variables d'environnement.

## Prérequis MongoDB : replica set

Le bot utilise des **transactions multi-documents** (voyages, craft, inventaire…).
MongoDB ne les supporte que sur un **replica set** (ou un cluster shardé), pas sur un `mongod` « standalone ».
Le MongoDB du `docker-compose.yml` est déjà configuré ainsi ; si vous utilisez le vôtre
(`MONGODB_HOST=host.docker.internal` pour une instance sur la machine hôte), activez `replication.replSetName`
puis initialisez-le avec `rs.initiate()`.

## Variables d'environnement

| Variable | Obligatoire | Défaut |
|---|---|---|
| `DISCORD_TOKEN` | oui | – |
| `MONGODB_USER` | oui | – |
| `MONGODB_PASSWORD` | oui | – |
| `MONGODB_HOST` | non | `127.0.0.1` (`mongo` dans docker-compose) |
| `MONGODB_PORT` | non | `27017` |
| `MONGODB_AUTH_SOURCE` | non | `admin` |

Si une variable obligatoire manque, ou si MongoDB est injoignable, le bot s'arrête au démarrage
avec un message d'erreur explicite.

## Développement

```bash
cargo run                 # nécessite un .env (ou des variables d'environnement) et un MongoDB
cargo test                # tests unitaires (sans base de données)
cargo test -- --ignored   # tests d'intégration : exigent MongoDB (et DISCORD_TOKEN pour le test du bot)
```

`Cargo.lock` est versionné : les builds (et la CI, via `--locked`) sont reproductibles.

## Intégration continue

`.github/workflows/ci.yml` exécute `cargo check`, `cargo test` et un build de l'image Docker à chaque push et pull request.
