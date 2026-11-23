# Le lancement : la Genèse en cartes de papier, puis l'arrivée dans le monde
: > "$OUT/jeu.log"
hc_launch --seed ${GRAINE:-4242}
hc_series genese 10 15
hc_key Escape; sleep 0.8; hc_key Escape
sleep 30; hc_shot arrivee
hc_cmd "/time set noon"; hc_cam 7; hc_shot arrivee-tps; hc_cam 0
hc_key semicolon; sleep 5; hc_shot carte; hc_key semicolon; sleep 1
hc_key c; sleep 3; hc_shot roue-competences; hc_key c
