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

#### Contrôles et Interface du prototype :
- **Bouton Écran `[NOUVELLE PLANETE [G]]`** : Cliquer directement à la souris sur le bouton interactif à l'écran pour générer une nouvelle planète avec des continents et mers uniques.
- **Bouton Écran `[AUTO-ROTATION]`** : Activer ou désactiver l'orbite continue.
- **Bouton Écran `[REINITIALISER VUE]`** : Recentrer la caméra sur la vue par défaut.
- **Clic Gauche / Clic Droit + Déplacement (Drag & Drop)** : Rotation orbitale fluide de la caméra autour de la planète.
- **Molette de la souris (Scroll)** : Zoom avant et dézoom arrière dynamique.
- **Raccourcis clavier** : Touches `[G]` (nouvelle graine), `[Espace]` (auto-rotation), `[R]` (reset caméra), `[Échap]` (quitter).

### Lancer les tests unitaires :
```bash
cargo test
```
