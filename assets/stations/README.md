# Établis et recettes

## Établis

Chaque fichier `*.json` de ce dossier décrit un établi. Son nom (sans `.json`) en est l'identifiant, celui que citent les recettes. Un établi nomme le bloc de chacun de ses niveaux : poser ce bloc, c'est poser l'établi à ce niveau. Un clic droit sur lui ouvre l'écran d'inventaire avec ses recettes (accroupi, le clic droit pose un bloc contre lui comme d'habitude). Un établi d'un niveau fait aussi les recettes des niveaux en dessous, et les recettes faites à la main sont toujours là.

```json
{
  "name": "Forge astrale",
  "description": "Un métal commun y devient fantastique.",
  "levels": ["astralForge", "ultimateAstralForge"]
}
```

| Champ | Rôle |
|---|---|
| `name`, `description` | Nom et description de l'établi. Chaque niveau s'affiche sous le nom de son bloc. |
| `levels` | Bloc de chaque niveau (identifiant de sa fiche dans `assets/blocks/`), du niveau 1 au dernier. Un bloc ne sert qu'à un niveau d'un établi. |

| Établi | Niveaux | Ce qui s'y fait |
|---|---|---|
| (à la main) | | planches, bâtons, corde, tofu, l'établi lui-même |
| `workbench` | 1 : `workbench` (Établi) | outils, armes et armures de bois, d'osier, de peau, d'os, de silex et de métal ; charbon, diamant brut, pierre taillée, panneaux de bois ; fourneau, tour de lapidaire |
| `furnace` | 1 : `furnace` (Fourneau) | lingots (cuivre, étain, bronze, fer, acier), viandes cuites et plats |
| `lapidary` | 1 : `lapidaryTower` (Tour de lapidaire) | gemmes taillées (ambre, aigue-marine, diamant, lapis, topaze), forge astrale |
| `astralForge` | 1 : `astralForge` (Forge astrale), 2 : `ultimateAstralForge` (Forge astrale ultime) | niveau 1 : mithril, orichalque, adamantine, éclats de vide et leur équipement, forge ultime ; niveau 2 : étherium et son équipement |

## Recettes

Une recette s'écrit dans la fiche de ce qu'elle fabrique, objet (`assets/items/`) ou bloc (`assets/blocks/`), dans le champ `recipes` : une liste, car un même objet peut se fabriquer de plusieurs façons.

```json
"recipes": [
  {"station": "workbench", "ingredients": {"bronzeIngot": 3, "stick": 2, "rope": 1}, "repair": {"bronzeIngot": 1}},
  {"ingredients": {"oakTrunk|birchTrunk|pineTrunk": 1}, "count": 4}
]
```

| Champ | Rôle |
|---|---|
| `station` | Établi où elle se fait (identifiant de sa fiche dans ce dossier). Absent : à la main, depuis l'inventaire (`E`). |
| `level` | Niveau d'établi demandé, 1 par défaut. |
| `ingredients` | Ce qu'il faut : identifiant d'un bloc ou d'un objet, et combien. `"a\|b\|c"` accepte l'un ou l'autre (et les mélange au besoin). |
| `count` | Exemplaires fabriqués, 1 par défaut. |
| `repair` | Ce qu'il faut, en plus de l'objet usé, pour le remettre à neuf au même établi. Pas encore utilisé : les objets ne s'usent pas encore. |

Une recette invalide (établi, niveau ou ingrédient inconnu) est signalée dans la console au lancement, puis ignorée. Les ingrédients sont pris dans le sac d'abord, puis dans la barre d'objets ; la recette ne se fait que si ce qu'elle donne trouve sa place.

Les 455 recettes actuelles viennent du module `HeroCrafting` de la branche `develop`. Le script `tools/importer_recettes.py` les reprend dans les fiches ; `tools/importer_blocs_objets.py` garde les recettes des fiches qu'il réécrit.
