# Une ville de chaque culture : au sol, en troisième personne et vue du ciel
# (seules les quatre grandes cultures bâtissent des villes ; CULTURES=... pour en choisir)
for c in ${CULTURES:-humain nain elfeBois hautElfe}; do
    hc_start --seed ${GRAINE:-4242} --visiter ville 1 --culture "$c"
    if hc_found; then hc_tour "ville-$c"; else echo "pas de ville $c"; fi
    hc_quit
done
