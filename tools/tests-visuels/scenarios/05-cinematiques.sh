# Les cinématiques ajoutées, jouées à la commande
hc_start --seed ${GRAINE:-4242} --visiter temple
hc_cmd "/cinematic chronique-du-roi"; hc_series chronique 8 18; hc_key Escape; sleep 0.8; hc_key Escape; sleep 3
for c in l-eclat:6:3 l-eveil:4:2 la-porte-cede:3:2 le-monstre-du-neant:3:1.5 l-appel-du-cristal:2:1.5 fin-de-la-vision:6:3 reveil-au-present:5:3; do
    IFS=: read -r name n step <<<"$c"
    hc_cmd "/cinematic $name"; hc_series "$name" "$n" "$step"; sleep 4
done
hc_quit
