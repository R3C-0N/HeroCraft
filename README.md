# HeroCraft

Un RPG Voxel multijoueur sur une planète entière générée procéduralement, écrit en Rust avec WGPU.

## Architecture

Le projet est structuré sous forme de **Cargo Workspace** modulaire :

- `crates/common` : Types partagés (blocs, RPG, protocoles réseau, maths planétaires).
- `crates/voxel_engine` : Structures de données des chunks (32x32x32), palette compression, maillage glouton (Greedy Meshing).
- `crates/world` : Génération procédurale de planète (Cubed-Sphere, climats, biomes, grottes, villes, villages et ruines).
- `crates/server` : Serveur de jeu dédié autonome (60 Hz tickrate, autorité serveur).
- `crates/client` : Client graphique (moteur de rendu WGPU / Bevy, caméra, interface).

## Compilation et Lancement

### Jouer à plusieurs :
Tout passe par **`Échap > Multijoueur`** :
- **Ouvrir au réseau local** : un serveur démarre dans le jeu sur le monde en cours (il reprend et enregistre sa sauvegarde `saves/monde_<graine>/`), et vous y restez connecté. Les autres joueurs du réseau local voient la partie dans leur liste.
- **Parties sur le réseau local** : les parties ouvertes (ou les serveurs dédiés) du réseau local s'affichent d'elles-mêmes (annonces UDP sur le port 7778). Un clic sur `Rejoindre` suffit.
- **Rejoindre par adresse** : `ip` ou `ip:port` (port 7777 par défaut).
- **Se déconnecter** / **Fermer la partie** : retour au monde solo.

À la connexion, le serveur envoie la graine de sa planète et les chunks déjà modifiés : chaque client génère le reste lui-même. Les autres joueurs apparaissent comme des silhouettes dorées surmontées de leur nom. Le serveur fait vivre les créatures et montre à chacun celles de son voisinage. En ligne, `G` (nouvelle planète) est désactivé.

### Lancer le serveur dédié :
```bash
cargo run --release --bin herocraft_server -- [--port 7777] [--seed 2026] [--name "Mon serveur"] [--no-lan]
```
Le serveur écoute en TCP (port 7777 par défaut) à 60 TPS et s'annonce sur le réseau local (sauf `--no-lan`). Il fait autorité sur les blocs : il valide chaque bloc cassé ou posé (portée, contenu), l'enregistre dans `saves/serveur_monde_<graine>/` et le diffuse à tous les joueurs. Il relaie aussi leurs positions. Tapez `stop` dans sa console pour l'arrêter en enregistrant le monde.

### Lancer le prototype client interactif :
```bash
cargo run --release --bin herocraft_client
```
Sans `--release`, le code du jeu est compilé sans optimisation : plus rapide à compiler, mais il tourne 5 à 10 fois plus lentement (les créatures à elles seules y prennent ~5 ms par image au lieu de ~0,6 ms). Le jeu le rappelle dans la console au lancement.
Pour rejoindre directement un serveur au lancement : `-- --connect 127.0.0.1:7777 --name Mat` (ou `HEROCRAFT_SERVER` et `HEROCRAFT_PSEUDO` dans `.env`).
Chaque partie solo, comme chaque appui sur « Nouvelle planète » (`G`), tire une graine au hasard, affichée dans la console. Pour retrouver une planète précise : `-- --seed 1234567890` (ou `HEROCRAFT_SEED` dans `.env`).

#### Configuration locale (`.env`) :
Copier `.env.example` en `.env` (non versionné). Les variables déjà définies dans l'environnement restent prioritaires.
- **`HEROCRAFT_MONITOR`** : écran où s'ouvre la fenêtre, par son nom Windows (`DISPLAY1`, `DISPLAY2`, `DISPLAY3`…). Sous WSLg, la correspondance avec l'écran Windows est retrouvée via `powershell.exe`, et la fenêtre passe par X11, car Wayland ne permet pas de la placer.

#### Végétation :
Le générateur pose la végétation selon le biome : chênes, bouleaux et bosquets roux en forêt, sapins en taïga et sur les alpages, grands arbres et sous-bois touffus dans la jungle, acacias en savane, cactus au désert, palmiers sur les plages chaudes, buissons un peu partout, et quelques bosquets dans les plaines. Au sol poussent des herbes hautes, des massifs de fleurs d'une même espèce dans les prairies, des iris et de la lavande en altitude et dans la toundra, des champignons sous les arbres et des buissons morts dans les régions sèches. Tout se déduit de la graine, sans rien enregistrer : un arbre ne déborde que de 3 blocs de son tronc, au plus un par case de 6 × 6 blocs. On traverse les herbes et les fleurs (un bloc posé en visant une herbe haute prend sa place) ; on bute contre les troncs et les feuillages, et on peut marcher dessus. Le relief lointain ne montre pas encore les arbres : ils apparaissent avec les chunks voxel.

#### Créatures :
Les 19 créatures du bestiaire (reprises du module `HeroBestiary` de la branche `develop`) sont dessinées avec leurs modèles glTF animés et leurs textures. Lapins, faisans, cerfs, loups, ours, sangliers, vaches et mouflons peuplent la planète selon le biome, sur l'herbe, la terre, le sable ou la neige : chaque zone de 32 × 32 blocs reçoit une fois pour toutes la population que lui destine la graine du monde, et plus rien n'apparaît ensuite. Une bête tuée ne revient pas. Les créatures des profondeurs et des mondes démoniaques n'apparaissent pas encore d'elles-mêmes. Toutes se promènent au hasard : elles franchissent les marches d'un bloc, évitent l'eau, les murs et les à-pics, et tombent dans les trous creusés sous elles. Leurs pattes bougent à la vitesse où elles avancent. Loin de tout joueur, une zone s'endort : ses bêtes ne bougent plus et reprennent là où elles étaient quand un joueur revient. La population (zones déjà peuplées, bêtes, positions, vie) est enregistrée dans `creatures.json`, dans la sauvegarde du monde. En solo, elles vivent dans le jeu et s'arrêtent avec le menu ; en ligne, c'est le serveur qui les fait vivre. Chaque espèce est décrite par un fichier de `assets/creatures/` : ajouter une bête, c'est ajouter un fichier et son modèle. Voir [assets/creatures/README.md](assets/creatures/README.md).

#### Villes, villages et ruines :
La planète porte des villes fortifiées, des villages et des ruines, posés par le générateur selon la graine : chaque face est découpée en cases de 128 × 128 blocs, et chaque case reçoit une ville, un village, des ruines ou rien, sur un site qui s'y prête (terre ferme, relief assez plat, biome). Une graine donne près d'une centaine d'établissements, toujours les mêmes, pour le client, le serveur et tous les joueurs en ligne.
- **Villes** : un damier de rues pavées entre des remparts crénelés, une tour à chaque angle, une porte au milieu de chaque côté, une grand-place avec sa fontaine, des lampadaires aux carrefours et une trentaine de maisons de briques ou de planches, souvent à étage (une échelle y monte).
- **Villages** : un puits couvert sur une petite place, deux à quatre chemins de gravier bordés de maisons, des lampadaires et des champs irrigués (blé, carottes, pommes de terre, courges).
- **Ruines** : temple sur podium aux colonnes brisées et à l'autel orné d'un cristal, vieille tour ronde éventrée, ou hameau de maisons effondrées.

Les matériaux suivent le climat : maisons à toits plats blanchis à la chaux dans les déserts et les savanes, rondins de pin et toits de pavés dans la taïga et la toundra, planches et toits d'escaliers ailleurs. Chaque bâtiment a un plancher plat : des fondations comblent la pente, et le relief est dégagé au-dessus. Rues, champs et remparts suivent le terrain. Tout se casse et se construit comme le reste du terrain. Les structures ne se voient que dans la zone de blocs autour du joueur, pas encore sur le relief lointain.
Pour aller voir : `-- --visiter ville` (ou `village`, `ruines`, suivi au besoin d'un numéro : `--visiter village 3`) fait apparaître au bord de l'établissement de ce genre. `cargo run -p herocraft_world --release --example settlements -- <graine>` les liste tous, et `-- <graine> <n°>` dessine le plan de l'un d'eux vu de dessus.

#### Blocs et objets :
Les 135 blocs et 523 objets des modules de la branche `develop` (`CoreAssets`, `HeroSurvival`, `HeroCrafting`, `HeroEquipment`, `HeroBestiary`…) sont décrits chacun par une fiche JSON, lue au lancement : `assets/blocks/` pour les blocs (nom, couleurs, textures, forme, passage, son, ce qu'il donne une fois cassé), `assets/items/` pour les objets (nom, description, taille de pile, icône, caractéristiques). Ajouter un bloc ou un objet, c'est ajouter une fiche. Fleurs, herbes, pousses et torches sont dessinées en deux panneaux croisés qu'on traverse ; le verre et la glace laissent voir les faces derrière eux. Les objets se rangent dans l'inventaire mais ne servent à rien pour l'instant (pas encore de combat, d'artisanat ni de nourriture). Voir [assets/blocks/README.md](assets/blocks/README.md) et [assets/items/README.md](assets/items/README.md).

#### Blessures et mort :
Une chute de plus de 5 blocs blesse : l'écran rougit, ses bords s'assombrissent de rouge et la vue penche un instant, avec un souffle coupé. Sous 30 % de vie, les bords de l'écran battent en rouge. À la mort, la vue s'effondre au sol et un écran « Vous êtes mort » donne la cause ; le bouton `Réapparaître` (ou `Espace`) ramène au point d'apparition, jauges pleines, après 2 secondes.

#### Contrôles du prototype :
Toutes les touches se changent dans `Échap > Commandes` (clic sur une touche, puis appui sur la nouvelle). Elles sont enregistrées dans `config/controles.json`.
- **Souris** : en jeu au sol, le curseur est capturé et la souris oriente le regard. Le menu Échap et la vue satellite le font réapparaître.
- **`Z Q S D`** (AZERTY, `W A S D` en QWERTY) : se déplacer. **`Ctrl`** : sprinter.
- **`Espace`** : sauter (1,3 bloc de haut, un peu plus d'une demi-seconde en l'air : de quoi franchir une marche d'un bloc). **Double `Espace`** : s'envoler ou se poser ; en vol, `Espace` monte et **`Maj`** descend (se poser au sol termine le vol).
- **Clic gauche** (maintenu) : casser le bloc visé par le réticule (son contour est surligné). Il se fissure peu à peu, plus ou moins vite selon sa matière (0,3 s pour les feuilles ou la neige, 0,5 s pour la terre, 1,5 s pour la pierre, 5 s pour l'obsidienne ; les autres blocs selon la dureté de leur fiche), puis tombe au sol en petit cube, attiré puis ramassé dans l'inventaire quand on passe à moins de 2,5 blocs (l'herbe donne de la terre, l'eau et la lave ne se ramassent pas ; un objet oublié disparaît au bout de 5 minutes, et un inventaire plein le laisse au sol). **Clic droit** : poser contre la face visée le bloc de la case choisie de la barre d'objets. Garder le clic droit enfoncé répète la pose ; garder le clic gauche enfoncé enchaîne les blocs. Les blocs modifiés sont enregistrés dans `saves/monde_<graine>/chunks/`.
- **Barre d'objets** (en bas de l'écran) : **`1`** à **`9`** ou la **molette** choisissent la case. **`E`** ouvre l'inventaire (27 cases de sac + la barre, piles de 64) : clic gauche pour prendre, poser, fusionner ou échanger une pile, clic droit pour en prendre la moitié ou poser un seul bloc. À côté, le **catalogue** liste tous les blocs et objets, par rubrique ou par recherche : clic gauche pour en prendre une pile, clic droit pour un seul. L'inventaire est enregistré avec le monde (`saves/monde_<graine>/inventaire.json`) ; en ligne, un inventaire à part est gardé pour la planète du serveur (`saves/en_ligne_<graine>/`).
- **`Alt` + molette** : zoom continu de la vue, du sol à l'orbite. En vue satellite, la molette seule suffit et clic + glisser tourne la planète.
- **`F`** : vue au sol / satellite. **`R`** : réinitialiser la vue. **`G`** : nouvelle planète. **`T` / `C`** : filtres plaques tectoniques / courants marins.
- **`Échap`** : menu de pause. **`F9`** : rapport de profilage.

#### Son :
Pas (selon le bloc sous les pieds : herbe, terre, pierre, sable, neige, bois, eau), saut, réception, blessure, coups portés au bloc qu'on casse, blocs cassés et posés (aussi ceux des autres joueurs proches), et ambiance : vent qui forcit en altitude, oiseaux le jour, grillons la nuit, étouffés sous un plafond. Les sons sont synthétisés au lancement, sans fichier audio. Les volumes (général, bruitages, ambiance) se règlent dans `Échap > Options > Son` et sont enregistrés dans `config/audio.json`. Sans sortie audio, le jeu tourne en silence.

Sous Linux et WSL, la compilation demande les en-têtes ALSA, une fois pour toutes :
```bash
sudo apt install libasound2-dev pkg-config
```
Le son passe par PulseAudio (ou PipeWire), ce que WSLg fournit d'office. Pour écouter tous les sons hors du jeu : `cargo test -p herocraft_client ecouter_les_sons -- --ignored --nocapture` (ou `exporter_les_sons` pour les écrire en WAV dans `target/sons/`).

### Profiler les performances :
```bash
cargo run --release --features profiling --bin herocraft_client
```
Le rapport est écrit dans `./profiling/` à la fermeture (ou avec `[F9]`). Voir [docs/PROFILAGE.md](docs/PROFILAGE.md).

### Banc d'essai automatique (FPS par fonctionnalité) :
```bash
cargo run --release --bin herocraft_client -- --benchmark
```
Le jeu joue seul une suite de phases d'environ 11 s (ombres, nuages, rayons, bloom, créatures, distance LOD, vues, marche, vol), puis écrit un rapport dans `./benchmark/` et se ferme. Voir [docs/BANC_ESSAI.md](docs/BANC_ESSAI.md).

### Lancer les tests unitaires :
```bash
cargo test
```

### Compilation rapide :
La compilation incrémentale est active : après une modification, `cargo build` ne recompile que ce qui a changé (environ 1,5 s au lieu de 5 s). Garder Rust à jour (`rustup update`) : depuis Rust 1.90, l'éditeur de liens `rust-lld`, bien plus rapide, est utilisé par défaut sous Linux et WSL.

**Compilation depuis Windows sur un partage WSL** (`\\wsl.localhost\...`, par exemple depuis un IDE Windows) : ce système de fichiers ne supporte pas les verrous de la compilation incrémentale. La désactiver côté Windows seulement, une fois pour toutes, dans PowerShell :
```powershell
setx CARGO_INCREMENTAL 0
```
Cette variable n'est pas transmise à WSL, où la compilation incrémentale reste active. Compiler depuis un terminal WSL reste le plus rapide.
