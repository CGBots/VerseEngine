# syntax=docker/dockerfile:1

# ---------- Build ----------
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
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=build /app/target/release/RpBot_reborn ./RpBot_reborn
COPY translations translations

# Aucun secret dans l'image : DISCORD_TOKEN et MONGODB_* sont fournis à l'exécution
# (variables d'environnement, voir docker-compose.yml).
USER 65534:65534
ENTRYPOINT ["./RpBot_reborn"]
