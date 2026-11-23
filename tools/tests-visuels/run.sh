#!/usr/bin/env bash
# Lance un scénario de test dans l'image : ./run.sh scenarios/NOM.sh
# Les captures arrivent dans shots/NOM/, les sauvegardes du jeu dans work/NOM/ (repartent de
# zéro à chaque scénario, avec le héros déjà créé de modele/).
set -euo pipefail
cd "$(dirname "$0")"
SCENARIO="$1"
NAME=$(basename "$SCENARIO" .sh)
rm -rf "shots/$NAME" "work/$NAME" 2>/dev/null || sudo rm -rf "shots/$NAME" "work/$NAME"
mkdir -p "shots/$NAME" "work/$NAME"
cp -r modele/. "work/$NAME/"
docker run --rm --cpus=10 --shm-size=1g --user "$(id -u):$(id -g)" -e HOME=/tmp -e CULTURES -e GRAINE -e CHARGEMENT \
    -v "${SRC:-$HOME/app/HeroCraft}":/src:ro \
    -v herocraft-target:/target:ro \
    -v "$PWD":/tests:ro \
    -v "$PWD/shots/$NAME":/shots \
    -v "$PWD/work/$NAME":/work/run \
    -w /work/run \
    herocraft-test bash -c "ln -sfn /src/assets assets; source /tests/lib.sh; hc_screen; source /tests/$SCENARIO; hc_quit"
