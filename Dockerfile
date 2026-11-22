# syntax=docker/dockerfile:1

# ---------- Azgaar (E9) : version figée, compilée pour être servie isolée sous /azgaar/ ----------
FROM node:22-bookworm-slim AS azgaar

RUN apt-get update \
    && apt-get install -y --no-install-recommends git ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /azgaar
COPY web/azgaar/ ./tools/
RUN AZGAAR_WORK=/azgaar/src ./tools/build.sh /azgaar/out

# ---------- Front de l'activité Discord (D1 : même image que le bot) ----------
FROM node:22-bookworm-slim AS web

WORKDIR /web
# Dépendances seules : couche mise en cache tant que le lockfile ne change pas.
COPY web/activity/package.json web/activity/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/activity/ ./
# Copié dans le build par Vite (public/) : servi sous /azgaar/ avec sa CSP `sandbox`.
COPY --from=azgaar /azgaar/out ./public/azgaar
# Identifiant PUBLIC de l'application (pas un secret) : il est intégré au JavaScript servi.
ARG VITE_DISCORD_CLIENT_ID=""
ENV VITE_DISCORD_CLIENT_ID=$VITE_DISCORD_CLIENT_ID
RUN npm test && npm run build

# ---------- Build du bot ----------
FROM rust:1-bookworm AS build

WORKDIR /app

# 1) Dépendances seules (couche mise en cache tant que Cargo.toml / Cargo.lock ne changent pas)
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

# 2) Code de l'application
COPY src src
RUN touch src/main.rs && cargo build --release --locked

# ---------- Runtime ----------
# Même base Debian que l'image de build (même glibc), sans toolchain Rust.
FROM debian:bookworm-slim

# Certificats racine : nécessaires pour les connexions TLS sortantes (Discord, MongoDB distant).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates python3 python3-venv \
    && rm -rf /var/lib/apt/lists/*

# Musique (MUSIC_SPEC.md § 3.3) : yt-dlp, yt-dlp-ejs et Deno, versions figées et vérifiées par
# empreinte. L'import YouTube reste coupé tant que MUSIC_YTDLP_ENABLED=true n'est pas fourni.
COPY docker/yt-dlp-requirements.txt /tmp/yt-dlp-requirements.txt
RUN python3 -m venv /opt/yt-dlp \
    && /opt/yt-dlp/bin/pip install --no-cache-dir --no-deps --require-hashes -r /tmp/yt-dlp-requirements.txt \
    && rm /tmp/yt-dlp-requirements.txt
ENV MUSIC_YTDLP_PATH=/opt/yt-dlp/bin/yt-dlp \
    MUSIC_YTDLP_DENO=/opt/yt-dlp/bin/deno

WORKDIR /app
COPY --from=build /app/target/release/RpBot_reborn ./RpBot_reborn
COPY translations translations
# Front statique, servi par le bot sur ACTIVITY_BIND quand ACTIVITY_ENABLED=true.
COPY --from=web /web/dist web/activity/dist

# Aucun secret dans l'image : DISCORD_TOKEN et MONGODB_* sont fournis à l'exécution
# (variables d'environnement, voir docker-compose.yml).
USER 65534:65534
ENTRYPOINT ["./RpBot_reborn"]
