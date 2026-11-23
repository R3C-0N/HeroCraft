#!/usr/bin/env bash
# Session de jeu pilotable à la main (pour mettre au point les scénarios) :
#   ./session.sh start NOM [ARGS DU JEU...]   lance l'écran virtuel et le jeu dans le conteneur hc-NOM
#   ./session.sh do NOM 'hc_cmd "/time set noon"; hc_shot midi'
#   ./session.sh stop NOM
set -euo pipefail
cd "$(dirname "$0")"
ACTION=$1; NAME=$2; shift 2
C="hc-$NAME"
case "$ACTION" in
start)
    docker rm -f "$C" >/dev/null 2>&1 || true
    mkdir -p "shots/$NAME" "work/$NAME"
    cp -rn modele/. "work/$NAME/"
    docker run -d --name "$C" --cpus=10 --shm-size=1g \
        -v "${SRC:-$HOME/app/HeroCraft}":/src:ro \
        -v herocraft-target:/target:ro \
        -v "$PWD":/tests:ro \
        -v "$PWD/shots/$NAME":/shots \
        -v "$PWD/work/$NAME":/work/run \
        -w /work/run \
        herocraft-test sleep infinity >/dev/null
    docker exec -d "$C" bash -c "ln -sfn /src/assets assets; Xvfb :99 -screen 0 1280x720x24 -nolisten tcp"
    sleep 1
    docker exec -d -e DISPLAY=:99 "$C" bash -c "/target/release/herocraft_client --solo $* >>/shots/jeu.log 2>&1"
    echo "session $C lancée"
    ;;
do)
    docker exec "$C" bash -c "source /tests/lib.sh; $1"
    ;;
stop)
    docker rm -f "$C" >/dev/null
    ;;
esac
