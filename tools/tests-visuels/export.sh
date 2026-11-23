#!/usr/bin/env bash
# Sort des captures en JPEG (base64 d'un tar.gz, entre deux marqueurs) pour les rapatrier
# ailleurs. ./export.sh DOSSIER/capture [DOSSIER/capture...]  (chemins sous shots/, sans .png)
# LARGEUR=800 et QUALITE=70 par défaut ; la sortie reste sous 190 000 caractères.
cd "$(dirname "$0")"
TMP=$(mktemp -d)
for path in "$@"; do
    dir=$(dirname "$path")
    mkdir -p "$TMP/$dir"
    docker run --rm -v "$PWD/shots":/i:ro -v "$TMP":/o herocraft-test \
        convert "/i/$path.png" -resize "${LARGEUR:-800}x" -quality "${QUALITE:-70}" "/o/$path.jpg"
done
DATA=$(tar -C "$TMP" -czf - . | base64 -w0)
rm -rf "$TMP"
if [ ${#DATA} -gt 185000 ]; then
    echo "TROP GROS (${#DATA} caractères) : moins de captures, ou LARGEUR/QUALITE plus bas"
    exit 1
fi
echo "==DEBUT=="
echo "$DATA"
echo "==FIN=="
# Remplissage : la sortie doit être longue pour être enregistrée dans un fichier
PAD=$(( 110000 - ${#DATA} ))
[ $PAD -gt 0 ] && head -c $(( PAD * 3 / 4 )) /dev/zero | base64 -w0
echo
