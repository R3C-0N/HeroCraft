# Habillage de l'interface

Textures et polices reprises du thème des menus de l'ancienne version Java (branche `develop`,
`engine/src/main/resources/org/terasology/engine/assets/`).

- **Textures** (`*.png`) : œuvres graphiques de Terasology (MovingBlocks), sous licence
  Creative Commons Attribution 4.0 International (voir `LICENSE_ARTWORK` sur la branche `develop`).
  `ornLeft`, `ornBar`, `ornGem` et `ornRight` sont découpées dans le bas de `titleMark.png`.
- **Grenze** (`fonts/Grenze-*.ttf`) : © 2019 The Grenze Project Authors, SIL Open Font License 1.1
  (`fonts/Grenze-OFL.txt`).
- **Uncial Antiqua** (`fonts/UncialAntiqua-Regular.ttf`) : SIL Open Font License 1.1
  (`fonts/UncialAntiqua-OFL.txt`).

L'ancien jeu utilisait ces polices en bitmap (`.fnt`) ; egui demande les fichiers TrueType, pris
chez Google Fonts.

Les textures `buttonMenu*` ont été retouchées : la pierre dorée à gauche est remplacée par le bois
voisin, pour ne garder que l'icône du bouton.
