#!/usr/bin/env bash
# Compile le client HeroCraft (release, sans LTO pour aller plus vite) dans l'image de test.
# Le dossier target et le registre Cargo restent dans des volumes Docker d'une fois sur l'autre.
set -euo pipefail
SRC="${SRC:-$HOME/app/HeroCraft}"
docker volume create herocraft-target >/dev/null
docker volume create herocraft-cargo >/dev/null
docker run --rm --cpus=10 \
    -v "$SRC":/src \
    -v herocraft-target:/target \
    -v herocraft-cargo:/usr/local/cargo/registry \
    -e CARGO_PROFILE_RELEASE_LTO=false \
    -e CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
    herocraft-test nice -n 10 cargo build --release -j 10 --bin herocraft_client
