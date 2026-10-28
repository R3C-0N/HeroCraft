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

### Jouer à plusieurs :
Tout passe par **`Échap > Multijoueur`** :
- **Ouvrir au réseau local** : un serveur démarre dans le jeu sur le monde en cours (il reprend et enregistre sa sauvegarde `saves/monde_<graine>/`), et vous y restez connecté. Les autres joueurs du réseau local voient la partie dans leur liste.
- **Parties sur le réseau local** : les parties ouvertes (ou les serveurs dédiés) du réseau local s'affichent d'elles-mêmes (annonces UDP sur le port 7778). Un clic sur `Rejoindre` suffit.
- **Rejoindre par adresse** : `ip` ou `ip:port` (port 7777 par défaut).
- **Se déconnecter** / **Fermer la partie** : retour au monde solo.

À la connexion, le serveur envoie la graine de sa planète et les chunks déjà modifiés : chaque client génère le reste lui-même. Les autres joueurs apparaissent comme des silhouettes dorées surmontées de leur nom. En ligne, `G` (nouvelle planète) est désactivé.

### Lancer le serveur dédié :
```bash
cargo run --release --bin herocraft_server -- [--port 7777] [--seed 2026] [--name "Mon serveur"] [--no-lan]
```
Le serveur écoute en TCP (port 7777 par défaut) à 60 TPS et s'annonce sur le réseau local (sauf `--no-lan`). Il fait autorité sur les blocs : il valide chaque bloc cassé ou posé (portée, contenu), l'enregistre dans `saves/serveur_monde_<graine>/` et le diffuse à tous les joueurs. Il relaie aussi leurs positions. Tapez `stop` dans sa console pour l'arrêter en enregistrant le monde.

### Lancer le prototype client interactif :
```bash
cargo run --bin herocraft_client
```
Pour rejoindre directement un serveur au lancement : `-- --connect 127.0.0.1:7777 --name Mat` (ou `HEROCRAFT_SERVER` et `HEROCRAFT_PSEUDO` dans `.env`).

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

### Compilation rapide :
La compilation incrémentale est active : après une modification, `cargo build` ne recompile que ce qui a changé (environ 1,5 s au lieu de 5 s). Garder Rust à jour (`rustup update`) : depuis Rust 1.90, l'éditeur de liens `rust-lld`, bien plus rapide, est utilisé par défaut sous Linux et WSL.

**Compilation depuis Windows sur un partage WSL** (`\\wsl.localhost\...`, par exemple depuis un IDE Windows) : ce système de fichiers ne supporte pas les verrous de la compilation incrémentale. La désactiver côté Windows seulement, une fois pour toutes, dans PowerShell :
```powershell
setx CARGO_INCREMENTAL 0
```
Cette variable n'est pas transmise à WSL, où la compilation incrémentale reste active. Compiler depuis un terminal WSL reste le plus rapide.
