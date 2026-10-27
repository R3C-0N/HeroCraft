# Profilage des performances

Le jeu est instrumenté par des points de mesure (`prof_scope!`) placés sur les fonctions
coûteuses : frame, mise à jour LOD, QuadTree, génération du monde, maillage des patchs
et des chunks voxels, upload GPU, rendu, interface egui.

Sans la feature cargo `profiling`, ces macros ne génèrent aucun code : le binaire normal
n'est pas affecté.

## Lancer une mesure

```bash
cargo run --release --features profiling --bin herocraft_client
```

Toujours en `--release` : une mesure en debug ne veut rien dire (le rapport l'indique
dans la ligne `build`).

Le rapport est écrit dans `./profiling/` (à la racine du dépôt) :

- à la fermeture de la fenêtre ou via le bouton **Quitter** du menu ;
- à tout moment avec **[F9]** (instantané, le jeu continue).

Un `Ctrl+C` dans le terminal tue le processus **sans** écrire le rapport.

## Fichiers produits

| Fichier | Contenu |
|---|---|
| `rapport_<horodatage>.txt` | Résumé texte : statistiques des frames (moyenne, p95, p99, max, nombre de frames > 16 / 33 / 50 / 100 ms), table de chaque portée par groupe de threads (appels, temps total, temps propre, p50/p95/p99/max), valeurs (nombre de feuilles, sommets, attente des tâches voxel…) et décomposition des 12 pires frames. |
| `trace_<horodatage>.json` | Chronologie des portées longues (≥ 50 µs) sur tous les threads. S'ouvre dans <https://ui.perfetto.dev> ou `chrome://tracing`. |

### Temps GPU

Si l'adaptateur supporte les *timestamp queries* (ligne `gpu_timestamps` du rapport), le groupe
`gpu` donne le temps réellement passé par le GPU sur chaque passe : `gpu::pass_3d` (découpé en
`sky`, `terrain` et `clouds_msaa_resolve` si les timestamps intra-passe sont supportés),
`gpu::shadows` (cascades du relief et carte des nuages), `gpu::post_bloom_tonemap` (bloom et composition HDR),
`gpu::pass_egui`, et le temps GPU total de la frame par vue (`[vue FPS]`, `[vue TPS]`,
`[vue orbite]`). Côté CPU, un `render::acquire_frame` élevé signifie que le CPU attend le GPU.

### Lire la table des portées

- **total** : temps inclusif (sous-portées comprises).
- **propre** : temps passé dans la portée elle-même, hors sous-portées mesurées. C'est la
  colonne de tri : elle désigne où le temps est réellement dépensé.
- **%mur** : temps propre rapporté à la durée de la session (et au nombre de threads du groupe).
- `cout_mesure_par_portee` : coût d'un point de mesure. Les fonctions appelées des millions de
  fois (`world::sample_*`) sont légèrement surévaluées de ce montant par appel.

## Ajouter un point de mesure

```rust
fn ma_fonction() {
    herocraft_common::prof_scope!("module::ma_fonction");
    // ...
}

// Fin de mesure explicite avant la fin du bloc
herocraft_common::prof_scope!("frame::etape" => guard);
etape();
drop(guard);

// Valeur numérique (le nom doit être un littéral)
herocraft_common::prof_value!("lod.feuilles", leaves.len());
```

Éviter d'instrumenter les fonctions de quelques dizaines de nanosecondes (bruit, `fbm_3d`) :
le coût de la mesure dominerait.
