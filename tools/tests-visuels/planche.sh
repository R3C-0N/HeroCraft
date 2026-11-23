#!/usr/bin/env bash
# Planche-contact de captures : ./planche.sh SORTIE capture1 capture2 ...  (chemins sous shots/,
# sans .png) -> shots/_planches/SORTIE.png, deux colonnes de 640 px, titre sous chaque capture
cd "$(dirname "$0")"
OUT=$1; shift
mkdir -p shots/_planches
ARGS=()
for p in "$@"; do ARGS+=("/s/$p.png"); done
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD/shots":/s herocraft-test \
    montage -background '#222' "${ARGS[@]}" \
    -tile 2x -geometry 640x360+3+3 "/s/_planches/$OUT.png"
