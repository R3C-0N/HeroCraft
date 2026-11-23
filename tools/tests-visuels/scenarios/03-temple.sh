# Les ruines d'un temple au présent, puis la Pierre du Néant posée et ses fragments
hc_start --seed ${GRAINE:-4242} --visiter temple
hc_tour temple
hc_cmd "/give voidStone 1"; hc_cmd "/give voidStoneFragment 3"
hc_key f; sleep 3; hc_shot inventaire-pierres; hc_key f; sleep 1
hc_cmd "/drop voidStone 1"; hc_cam 5; hc_look 0 150; sleep 2; hc_shot pierre-au-sol
hc_quit
