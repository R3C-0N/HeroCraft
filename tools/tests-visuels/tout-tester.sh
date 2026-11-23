#!/usr/bin/env bash
# Teste tout d'un coup : met le code à jour, compile, joue les scénarios et fait la galerie.
#   ./tout-tester.sh                 tous les scénarios
#   ./tout-tester.sh 01 04           seulement ceux-là
#   PAS_DE_MAJ=1 ./tout-tester.sh    sans git pull
# Les captures arrivent dans shots/<scénario>/, la galerie dans shots/index.html.
set -uo pipefail
cd "$(dirname "$0")"
SRC="${SRC:-$HOME/app/HeroCraft}"
if [ -z "${PAS_DE_MAJ:-}" ]; then
    (cd "$SRC" && GIT_SSH_COMMAND="ssh -o BatchMode=yes" git pull -q) || echo "git pull impossible, je teste le code tel quel"
fi
echo "== compilation"
./build.sh 2>&1 | tail -1
for s in scenarios/*.sh; do
    name=$(basename "$s" .sh)
    if [ $# -gt 0 ]; then
        keep=; for want in "$@"; do [[ "$name" == "$want"* ]] && keep=1; done
        [ -z "$keep" ] && continue
    fi
    echo "== $name ($(date +%H:%M))"
    start=$(date +%s)
    ./run.sh "$s" 2>&1 | grep -v "^capture" | sed 's/^/   /'
    echo "   $(ls shots/$name/*.png 2>/dev/null | wc -l) captures en $(( $(date +%s) - start )) s"
    grep -ah "ERROR\|panicked" "shots/$name/jeu.log" | head -5 | sed 's/^/   ! /'
done
./galerie.sh
echo "== fini : $PWD/shots/index.html"
