# Modules du jeu

Tous les modules du jeu vivent dans ce dépôt. Gradle inclut chaque dossier
de `modules/` qui porte un `build.gradle` et un `module.txt`
(`subprojects.settings.gradle.kts`) ; un module neuf doit aussi être ajouté
à la liste blanche de `/modules/*` dans `.gitignore`.

Les modules du jeu portent le préfixe `Hero` ; les autres sont des forks
d'upstream, qui ne reçoivent que des correctifs.

| Module | Contenu | Origine |
|---|---|---|
| `HeroCraft` | le jeu : assemble les modules `Hero`, le joueur, son HUD et son inventaire (surcharges de `Health` et `Inventory`), la limite de hauteur de construction | né du fork ; tiré de `CoreSampleGameplay`, importé de `CoreSampleGameplay-Evolve` à `54d808c` |
| `HeroCommon` | ce que plusieurs modules lisent sans dépendre les uns des autres : le marqueur du mode créatif | né du fork |
| `HeroEquipment` | outils, armes, armures, grades de récolte, grattage, usure et réparation | né du fork ; tiré de `CoreSampleGameplay` |
| `HeroSurvival` | faim, nourriture et plats, agriculture, exposition, brûlure dans les liquides | né du fork ; tiré de `CoreSampleGameplay` |
| `HeroCrafting` | stations, recettes, paliers, fourneau | né du fork ; tiré de `CoreSampleGameplay` |
| `HeroBestiary` | créatures, apparition, recul, dépeçage, viandes | né du fork (ex-`Bestiaire`) |
| `HeroCreative` | le mode créatif complet, et le gameplay qui démarre en créatif | né du fork (ex-`CreativeGameplay`, plus le créatif de `CoreSampleGameplay`) |
| `HeroCubeWorlds` | monde-cube rendu en sphère, terrain lointain | né du fork (ex-`CubeWorlds`) ; importé de `CubeWorlds-Evolve` à `cc70066` |
| `CoreRendering` | passes de rendu, ouvertes au monde courbe | fork de `Terasology/CoreRendering` ; importé à `6961156` |
| `CoreWorlds` | générateurs plats et biomes de base | fork de `Terasology/CoreWorlds` ; importé à `e43e41a` |
| `CoreAssets` | blocs et textures de base | fork de `Terasology/CoreAssets` ; importé à `0ee3169` |
| `Minimap` | minicarte | fork de `Terasology/Minimap` ; importé à `56ea7c2` |
| `WorkstationCrafting` | registre de recettes, chaleur, procédés | fork de `Terasology/WorkstationCrafting` ; importé à `13d2546` |

Les dépendances entre modules du jeu vont dans un seul sens (`A ← B` : `B`
dépend de `A`) :

```
HeroCommon ← HeroEquipment ← HeroSurvival ← HeroBestiary ← HeroCrafting ← HeroCraft ← HeroCreative
                               ↑                                            ↑
                         HeroCubeWorlds ────────────────────────────────────┘
```

Un système qui a besoin d'un module placé plus à droite n'a pas sa place
plus à gauche : on le déplace, ou on pose ce qu'ils partagent dans
`HeroCommon`.

Les dépôts `*-Evolve` d'où viennent les modules importés sont l'archive de
leur historique jusqu'au commit indiqué ; l'historique complet est aussi
présent ici. Les modules sans dossier ici (`Health`, `Inventory`,
`BiomesAPI`…) sont des binaires récupérés par Gradle.

Pour reprendre une évolution d'un upstream Terasology :

```bash
git subtree pull --prefix=modules/CoreRendering \
    https://github.com/Terasology/CoreRendering.git develop
```
