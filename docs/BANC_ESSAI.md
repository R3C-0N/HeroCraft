# Banc d'essai automatique

Pour savoir ce qui coûte des FPS, le jeu peut se tester tout seul : il joue une suite de phases
et, dans chacune, ne change qu'une seule chose par rapport à la référence.

```bash
cargo run --release --bin herocraft_client -- --benchmark
```

Environ 3 minutes et demie. La souris reste libre : la fenêtre peut être déplacée sur un autre
écran et vous pouvez faire autre chose à côté, tant que la fenêtre n'est pas réduite (un autre
programme gourmand fausse quand même la mesure). À la fin, le rapport est écrit dans `./benchmark/`
et le jeu se ferme.

Toujours en `--release` : en debug, le code du jeu tourne 5 à 10 fois plus lentement, et le
rapport le signale en tête.

## Ce que fait le banc d'essai

- Toujours la même planète (graine 2026, ou celle de `--seed`) et le même point de départ, au
  soleil de midi qui reste fixe.
- V-Sync et limite de FPS coupées, joueur invulnérable, rien n'est enregistré dans la sauvegarde.
- Une mise en route de 20 s, non mesurée : le relief, les nuages et les textures se chargent.
- Puis 19 phases, chacune avec 3 s de chauffe et 8 s de mesure. Avant chaque phase, le joueur
  revient au point de départ et la population de créatures est tirée de nouveau, la même chaque
  fois. Pendant la mesure, la tête fait un tour complet : toutes les directions comptent.

| Phase | Ce qui change |
|---|---|
| Référence | tout activé, vue à la première personne |
| Sans ombres du relief | coupe aussi les ombres des nuages et les rayons, qui en dépendent |
| Sans ombres des nuages | |
| Sans nuages | |
| Sans rayons crépusculaires | |
| Sans bloom | |
| Créatures simulées, non dessinées | coût de l'animation par os, du maillage et du dessin |
| Sans créatures | ni simulation, ni dessin |
| Distance LOD courte / élevée | |
| Minimum | tous les effets et les créatures coupés |
| Qualité moyenne, qualité basse | les préréglages du menu Options > Affichage |
| Beaucoup de créatures | 60 bêtes de plus autour du joueur |
| Vue à la troisième personne, vue satellite | |
| Marche, vol rapide | le relief se charge en continu |
| Référence (contrôle) | la référence, mesurée de nouveau à la fin |

Durées réglables : `--benchmark-chauffe 3 --benchmark-mesure 8` (en secondes).

## Lire le rapport

`rapport_<horodatage>.md` contient :

- **Images par phase** : FPS, durée moyenne d'une image, p50/p95/p99/max, part des images au-delà
  de 16,7 ms (sous 60 FPS), et l'écart avec la référence.
- **Ce que coûte chaque fonction** : le temps gagné par image en la coupant, de la plus chère à la
  moins chère. Les deux références (au début et à la fin) donnent la marge de bruit : un écart plus
  petit que l'écart entre elles ne veut rien dire.
- **Temps CPU par étape** : caméra et joueur, simulation des créatures, animation et maillage des
  créatures, LOD et envoi au GPU, interface, rendu. Le « rendu » comprend l'attente d'une image libre
  de la chaîne de présentation : s'il domine, c'est le GPU qui limite.
- **Temps GPU par passe** (si la carte fournit les horodatages GPU) : ombres, passe 3D (relief,
  ciel, nuages), rayons, bloom, interface, et l'image entière.

`mesures_<horodatage>.csv` reprend les mêmes chiffres (séparateur `;`) pour un tableur.

## Simulation des créatures seule

La simulation tourne aussi sur le serveur, sans fenêtre. Pour la mesurer seule :

```bash
cargo run -p herocraft_world --release --example creature_bench -- [vitesse en blocs/s] [secondes] [x] [z]
```
