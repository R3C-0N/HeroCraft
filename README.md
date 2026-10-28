# HeroCraft

Un RPG Voxel multijoueur sur une planète entière générée procéduralement, écrit en Rust avec WGPU.

## Architecture

Le projet est structuré sous forme de **Cargo Workspace** modulaire :

- `crates/common` : Types partagés (blocs, RPG, protocoles réseau, maths planétaires).
- `crates/voxel_engine` : Structures de données des chunks (32x32x32), palette compression, maillage glouton (Greedy Meshing).
- `crates/world` : Génération procédurale de planète (Cubed-Sphere, climats, biomes, grottes).
- `crates/server` : Serveur de jeu dédié autonome (60 Hz tickrate, autorité serveur).
- `crates/client` : Client graphique (moteur de rendu WGPU / Bevy, caméra, interface).

## Compilation et Lancement

### Lancer le serveur dédié :
```bash
cargo run --bin herocraft_server
```

### Lancer le prototype client interactif :
```bash
cargo run --bin herocraft_client
```

#### Configuration locale (`.env`) :
Copier `.env.example` en `.env` (non versionné). Les variables déjà définies dans l'environnement restent prioritaires.
- **`HEROCRAFT_MONITOR`** : écran où s'ouvre la fenêtre, par son nom Windows (`DISPLAY1`, `DISPLAY2`, `DISPLAY3`…). Sous WSLg, la correspondance avec l'écran Windows est retrouvée via `powershell.exe`, et la fenêtre passe par X11, car Wayland ne permet pas de la placer.

#### Contrôles du prototype :
Toutes les touches se changent dans `Échap > Commandes` (clic sur une touche, puis appui sur la nouvelle). Elles sont enregistrées dans `config/controles.json`.
- **Souris** : en jeu au sol, le curseur est capturé et la souris oriente le regard. Le menu Échap et la vue satellite le font réapparaître.
- **`Z Q S D`** (AZERTY, `W A S D` en QWERTY) : se déplacer. **`Ctrl`** : sprinter.
- **`Espace`** : sauter. **Double `Espace`** : s'envoler ou se poser ; en vol, `Espace` monte et **`Maj`** descend (se poser au sol termine le vol).
- **Clic gauche** : détruire le bloc visé par le réticule (son contour est surligné). **Clic droit** : poser un bloc de pierre contre la face visée. Garder le bouton enfoncé répète l'action. Les blocs modifiés sont enregistrés dans `saves/monde_<graine>/chunks/`.
- **`Alt` + molette** : zoom continu de la vue, du sol à l'orbite. En vue satellite, la molette seule suffit et clic + glisser tourne la planète.
- **`F`** : vue au sol / satellite. **`R`** : réinitialiser la vue. **`G`** : nouvelle planète. **`T` / `C`** : filtres plaques tectoniques / courants marins.
- **`Échap`** : menu de pause. **`F9`** : rapport de profilage.

### Profiler les performances :
```bash
cargo run --release --features profiling --bin herocraft_client
```
Le rapport est écrit dans `./profiling/` à la fermeture (ou avec `[F9]`). Voir [docs/PROFILAGE.md](docs/PROFILAGE.md).

### Lancer les tests unitaires :
```bash
cargo test
```
