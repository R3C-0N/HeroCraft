# Outils des scénarios de test : écran virtuel, lancement du jeu, commandes, touches, captures.
# Sourcé dans le conteneur par run.sh (et session.sh).
export DISPLAY=:99
W=${W:-1280}
H=${H:-720}
OUT=${OUT:-/shots}
GAME=/target/release/herocraft_client
# Les commandes passent par ce fichier, que le jeu lit à chaque image
export HEROCRAFT_COMMANDES=/work/run/commandes.txt
GAME_PID=

hc_screen() {
    Xvfb :99 -screen 0 ${W}x${H}x24 -nolisten tcp >/dev/null 2>&1 &
    sleep 1
}

# hc_launch ARGS... : lance le jeu (solo) et attend sa fenêtre
hc_launch() {
    : > "$HEROCRAFT_COMMANDES"
    "$GAME" --solo "$@" >>"$OUT/jeu.log" 2>&1 &
    GAME_PID=$!
    for _ in $(seq 1 120); do
        WIN=$(xdotool search --name HeroCraft 2>/dev/null | head -1)
        [ -n "$WIN" ] && break
        sleep 1
    done
    echo "fenêtre : ${WIN:-introuvable}"
}

hc_quit() {
    [ -n "$GAME_PID" ] && kill "$GAME_PID" 2>/dev/null
    wait "$GAME_PID" 2>/dev/null
    GAME_PID=
}

# hc_start ARGS... : lance le jeu, passe la Genèse, attend que le monde soit chargé, midi
hc_start() {
    : > "$OUT/jeu.log"
    hc_launch --sans-genese "$@"
    sleep 8
    hc_cmd "/camera 0"
    sleep ${CHARGEMENT:-45}
    hc_cmd "/time set noon"
    hc_cmd "/time speed 0"
    sleep 3
}

hc_wait() { sleep "$1"; }

# hc_shot NOM : capture de l'écran entier dans $OUT/NOM.png
hc_shot() {
    import -window root "$OUT/$1.png" 2>/dev/null && echo "capture $1"
}

hc_focus() {
    WIN=$(xdotool search --name HeroCraft 2>/dev/null | head -1)
    [ -n "$WIN" ] && xdotool windowfocus --sync "$WIN" 2>/dev/null
    true
}

hc_key() { hc_focus; xdotool key --delay 150 "$@"; }

hc_click() { hc_focus; xdotool mousemove "$1" "$2"; sleep 0.4; xdotool mousedown 1; sleep 0.3; xdotool mouseup 1; sleep 0.5; }

# hc_cmd "/commande args" : exécutée par le jeu comme si elle avait été tapée dans le chat
hc_cmd() {
    echo "$1" >> "$HEROCRAFT_COMMANDES"
    sleep 1.5
}

# hc_chat "texte" : tapé au clavier dans le chat (pour tester le chat lui-même)
hc_chat() {
    hc_focus
    xdotool key Return; sleep 1.5
    xdotool type --delay 120 "$1"; sleep 1
    xdotool key Return; sleep 1.5
}

# hc_hold TOUCHE SECONDES : garde une touche enfoncée (marcher…)
hc_hold() {
    hc_focus
    xdotool keydown "$1"
    sleep "$2"
    xdotool keyup "$1"
}

# hc_look DX DY : tourne la vue de DX, DY pixels (400 px font environ un quart de tour)
hc_look() { hc_focus; xdotool mousemove_relative -- "$1" "$2"; }

# hc_cam DISTANCE [YAW] [PITCH] : caméra à DISTANCE blocs du héros
hc_cam() { hc_cmd "/camera $*"; sleep 3; }

# hc_tour NOM : vues d'un lieu (au sol, troisième personne, vue aérienne sous trois angles)
hc_tour() {
    hc_cam 0; hc_shot "$1-sol"
    hc_cam 7; sleep 3; hc_shot "$1-tps"
    hc_cam 0
    # Vue d'ensemble : le héros s'envole et recule, le regard toujours tourné vers le lieu
    hc_cmd "/fly"; hc_hold space ${MONTEE:-6}; hc_hold s ${RECUL:-3}
    hc_cam 20 '~' -35; sleep 4; hc_shot "$1-aerien"
    hc_cam 40 '~' -50; sleep 4; hc_shot "$1-aerien-2"
    hc_cam 70 '~' -65; sleep 5; hc_shot "$1-aerien-3"
    hc_cam 0; hc_cmd "/fly"
}

# hc_series NOM N PAS : N captures espacées de PAS secondes (cinématiques…)
hc_series() {
    for i in $(seq 1 "$2"); do sleep "$3"; hc_shot "$1-$(printf %02d "$i")"; done
}

# hc_found : vrai si la dernière visite a trouvé un établissement
hc_found() { ! grep -q "aucun établissement" "$OUT/jeu.log"; }
