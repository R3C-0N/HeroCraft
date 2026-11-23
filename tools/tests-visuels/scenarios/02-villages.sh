# Un village de chaque culture
for c in ${CULTURES:-humain nain elfeBois hautElfe demiOrque revenant sylphe gnome aasimar hommeBete}; do
    hc_start --seed ${GRAINE:-4242} --visiter village 1 --culture "$c"
    if hc_found; then hc_tour "village-$c"; else echo "pas de village $c"; fi
    hc_quit
done
