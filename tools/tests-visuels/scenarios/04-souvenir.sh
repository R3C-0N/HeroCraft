# La vision du temple : l'entrée (L'éclat), le soldat, l'éclat, l'éveil, puis la charge
hc_start --seed ${GRAINE:-4242} --visiter temple
hc_cmd "/vision"
hc_series eclat 8 3
sleep 5; hc_shot tutoriel-arrivee
hc_cam 10; hc_shot tutoriel-soldats; hc_cam 0
hc_key f; sleep 3; hc_shot tutoriel-sac; hc_key f; sleep 1
# L'éclat ramassé (raccourci : posé devant soi puis ramassé) : l'éveil, puis la charge
hc_cmd "/drop voidStoneFragment 1"; hc_hold w 2
hc_series eveil 4 3
hc_cam 12; sleep 2
hc_series charge 10 6
hc_key b; sleep 1
# Les combattants de l'assaut, appelés devant le héros
for s in soldatAllie soldatEnnemi capitaineEnnemi bruteDeGuerre championVille herautDuNeant monstreDuNeant; do
    hc_cmd "/summon $s 1"; sleep 4; hc_cam 6; hc_shot "creature-$s"; hc_cam 0
done
hc_cmd "/vision end"
hc_series retour 6 3
hc_quit
