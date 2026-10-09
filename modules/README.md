# Modules du jeu

Tous les modules du jeu vivent dans ce dépôt. Gradle inclut chaque dossier
de `modules/` qui porte un `build.gradle` et un `module.txt`
(`subprojects.settings.gradle.kts`) ; un module neuf doit aussi être ajouté
à la liste blanche de `/modules/*` dans `.gitignore`.

| Module | Contenu | Origine |
|---|---|---|
| `Bestiaire` | créatures, apparition, recul, dépeçage | né du fork |
| `CreativeGameplay` | gameplay créatif | né du fork |
| `CoreSampleGameplay` | le gameplay du jeu : fabrication, équipement, agriculture, faim, exposition, créatif, HUD | fork de `Terasology/CoreSampleGameplay`, réécrit ; importé de `CoreSampleGameplay-Evolve` à `54d808c` |
| `CubeWorlds` | monde-cube rendu en sphère, terrain lointain | né du fork ; importé de `CubeWorlds-Evolve` à `cc70066` |
| `CoreRendering` | passes de rendu, ouvertes au monde courbe | fork de `Terasology/CoreRendering` ; importé à `6961156` |
| `CoreWorlds` | générateurs plats et biomes de base | fork de `Terasology/CoreWorlds` ; importé à `e43e41a` |
| `CoreAssets` | blocs et textures de base | fork de `Terasology/CoreAssets` ; importé à `0ee3169` |
| `Minimap` | minicarte | fork de `Terasology/Minimap` ; importé à `56ea7c2` |
| `WorkstationCrafting` | registre de recettes, chaleur, procédés | fork de `Terasology/WorkstationCrafting` ; importé à `13d2546` |

Les dépôts `*-Evolve` d'où viennent les modules importés sont l'archive de
leur historique jusqu'au commit indiqué ; l'historique complet est aussi
présent ici. Les modules sans dossier ici (`Health`, `Inventory`,
`BiomesAPI`…) sont des binaires récupérés par Gradle.

Pour reprendre une évolution d'un upstream Terasology :

```bash
git subtree pull --prefix=modules/CoreRendering \
    https://github.com/Terasology/CoreRendering.git develop
```
