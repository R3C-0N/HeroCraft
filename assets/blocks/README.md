# Blocs

Chaque fichier `*.json` de ce dossier décrit un bloc. Son nom (sans `.json`) en est l'identifiant, celui qu'une autre fiche cite (par exemple dans `drop`). Les textures sont rangées dans `textures/`, les formes réutilisables (dalle, escalier…) dans `shapes/`.

Pour ajouter un bloc, il suffit d'ajouter sa fiche : le rendu du terrain, les collisions, les sons, l'inventaire et le serveur lisent tout dans ces fichiers. Un fichier invalide est signalé dans la console au lancement, puis ignoré. Un bloc s'obtient en le cassant, ou en le fabriquant selon les recettes de sa fiche (`recipes`, voir [assets/stations/README.md](../stations/README.md)). Les établis sont des blocs : leurs niveaux sont décrits dans `assets/stations/`.

Les 135 premiers blocs viennent des modules de la branche `develop` (`CoreAssets`, `HeroSurvival`, `HeroCrafting`, `HeroDungeons`). Le script `tools/importer_blocs_objets.py` les reprend : il convertit les fichiers `.block`, copie les textures et tire la couleur de chaque face de la moyenne de sa texture. Le relancer après une retouche des modules remet ces fichiers à jour sans changer les numéros ; les fiches écrites à la main (les dalles et escaliers) ne sont pas touchées.

## Exemple

```json
{
  "numeric_id": 61,
  "name": "Terre labourée",
  "description": "Prête pour les semis.",
  "category": "Cultures",
  "hardness": 9,
  "color": {"top": "#684425", "side": "#997c58"},
  "textures": {"top": "textures/TerreLabouree.png", "side": "textures/Dirt.png"},
  "sound": "dirt",
  "drop": "dirt"
}
```

## Champs

| Champ | Rôle |
|---|---|
| `numeric_id` | Numéro du bloc dans les chunks, le réseau et les sauvegardes, de 1 à 8191 (0 est l'air). **Ne plus le changer** une fois des mondes enregistrés : un bloc posé y est noté par ce numéro. Les numéros 1 à 13 sont ceux que pose le générateur de terrain (pierre, terre, herbe, sable, neige, eau, lave, tronc, feuilles, minerais de fer et d'or, cristal, obsidienne). La végétation qu'il pose (troncs, feuillages, cactus, herbes, fleurs, champignons) est cherchée par l'identifiant de sa fiche (`crates/world/src/vegetation.rs`) : renommer l'une de ces fiches la retire du monde. |
| `name`, `description` | Nom affiché et description (bulle de l'inventaire). |
| `category` | Rubrique (`Minéraux`, `Sols`, `Flore`, `Construction`…). |
| `hardness` | Dureté, celle des modules d'origine (1 pour une fleur, 14 pour la pierre). Le bloc se casse en un dixième de seconde par point de dureté (les blocs du terrain ont leurs propres temps). |
| `color` | Couleur des faces sans texture : `side` pour les côtés et le dessous, `top` pour le dessus (celle des côtés à défaut). `#rrggbb`, ou `#rrggbbaa` pour une face translucide. |
| `textures` | Textures PNG des faces, chemins depuis ce dossier : `side`, et au besoin `top` et `bottom`. 16 x 16 texels (une image plus grande est ramenée à 16 x 16, une bande d'images réduite à la première). Les texels transparents sont des trous (fleurs, feuillages, échelle). Elles couvrent les blocs du terrain et dessinent l'icône de l'inventaire. Les liquides gardent leur couleur. |
| `texture_tint` | Teinte appliquée aux textures grises (herbe, feuillages), `#rrggbb` : sur le dessus, et sur les côtés qui ont la texture du dessus. |
| `shape` | Forme du bloc : `cube` (par défaut), `plant` (deux panneaux croisés en diagonale, pour les fleurs, les herbes et les pousses), le nom d'une forme de `shapes/` (`dalle`, `escalier`, `panneau`, `poteau`, `colonne`, `torche`), ou des pavés décrits sur place (voir plus bas). Une plante ou une forme ne cache que ce que ses faces couvrent entièrement. |
| `rotation` | Orientation choisie à la pose, pour un bloc qui a une forme. `none` (par défaut) : toujours posé pareil. `horizontal` : il se tourne d'un quart de tour en quart de tour pour que l'avant de sa forme regarde le joueur (escaliers, échelle). `face` : il se couche contre la face visée, le dessous de sa forme collé au bloc visé (une dalle posée au sol est basse, sous un plafond haute, contre un mur verticale). |
| `upside_down` | Avec `rotation: horizontal` : `true` pour qu'il se pose à l'envers quand on vise le dessous d'un bloc ou la moitié haute d'un côté (escaliers). |
| `solid` | `false` : on le traverse (plantes, torche). Vrai par défaut. Un bloc qu'on traverse se pose aussi là où se tient le joueur. |
| `opaque` | `false` : il laisse voir les faces des blocs derrière lui (verre, glace, feuillages). Vrai par défaut. |
| `liquid` | `true` : un liquide (eau, lave). Il n'est ni solide, ni opaque, ni visé par le réticule, ni ramassé ; un bloc posé le remplace. |
| `replaceable` | `true` : une herbe folle (herbes hautes, buisson mort). Un bloc posé en la visant prend sa place, au lieu de se poser par-dessus. |
| `connected` | `true` : texture « connectée » (le verre). Son cadre n'est dessiné que sur les bords qui ne touchent pas un bloc du même genre : deux blocs côte à côte forment un seul panneau, encadré d'un seul tenant. Le dedans de la texture reste transparent, sauf ses reflets. |
| `light` | Lumière émise, de 0 à 15. Pas encore rendue. |
| `sound` | Matière des pas et des blocs cassés ou posés : `stone` (par défaut), `dirt`, `grass`, `sand`, `snow`, `wood` ou `water`. |
| `drop` | Ce que donne le bloc cassé : lui-même si le champ est absent, l'identifiant d'un autre bloc (l'herbe donne `dirt`), ou `null` pour rien. |
| `loot` | Butin tiré en plus de `drop` quand le bloc est cassé : une liste d'objets ou de blocs, chacun avec `count` (un nombre, ou `[min, max]` tiré au hasard ; 1 par défaut) et `chance` (probabilité de tomber, 1 par défaut). Exemple, les herbes hautes : `[{"item": "fibre", "chance": 0.5}, {"item": "grainesBle", "chance": 0.05}]`. |
| `recipes` | Recettes qui le fabriquent : établi et niveau, ingrédients, nombre fabriqué. Voir [assets/stations/README.md](../stations/README.md). |

## Formes

Un bloc qui n'est pas plein est fait de pavés, mesurés en seizièmes de bloc : `[x0, y0, z0, x1, y1, z1]`, de 0 à 16, `y` vers le haut. L'avant de la forme est le côté `z = 0` : c'est lui qui regarde le joueur quand le bloc s'oriente (`"rotation": "horizontal"`). Son dessous est le côté `y = 0` : c'est lui qui se colle au bloc visé quand le bloc se couche contre une face (`"rotation": "face"`).

Une forme qui sert à plusieurs blocs a sa fiche dans `shapes/`, citée par son nom (`"shape": "escalier"`) :

```json
{
  "name": "Escalier",
  "description": "Une marche basse à l'avant, une haute à l'arrière.",
  "boxes": [[0, 0, 0, 16, 8, 16], [0, 8, 8, 16, 16, 16]]
}
```

Une forme propre à un bloc se décrit sur place : `"shape": {"boxes": [[0, 0, 0, 16, 2, 16]]}`.

Tout en découle :

- **Dessin** : chaque face d'un pavé est dessinée, sauf celles collées à un autre pavé de la forme ou à un voisin qui les couvre. Une face montre le morceau de texture qu'elle occupe dans le bloc (le bas des côtés pour une dalle). Avec `"texture": "stretch"` (dans la fiche de la forme, ou à côté de `boxes`), chaque face montre la texture entière, étirée (la torche).
- **Voisins** : une face de la forme entièrement couverte (le dessous d'une dalle, le dos d'un escalier) cache la face du voisin collée contre elle, si le bloc est `opaque`.
- **Collisions et visée** : le joueur bute contre les pavés seuls, marche sur leur dessus, et monte sans sauter une marche d'un demi-bloc au plus (dalle, escalier). Le réticule vise les pavés ; un bloc posé contre une forme va dans la case voisine, comme contre un cube.
- **Orientation** : elle tient dans les trois bits hauts du numéro posé dans le monde (les fiches restent sous 8192) : quatre quarts de tour, à l'endroit ou à l'envers, ou l'une des six faces. Un bloc orienté se ramasse comme le bloc de sa fiche.

| Forme | Pavés | Blocs |
|---|---|---|
| `dalle` | moitié basse (couchée contre la face visée : basse, haute ou verticale) | dalles de planches, de pavés, de briques |
| `escalier` | dalle + marche haute à l'arrière (tourné vers le joueur, à l'envers possible) | escaliers de planches, de pavés, de briques |
| `panneau` | plaque de 2 seizièmes au fond | échelle |
| `colonne` | bloc aminci de deux seizièmes de chaque côté, à la mesure des textures du cactus | cactus |
| `torche` | bâton de 2 x 10 x 2, texture étirée | torche |
| `poteau` | poteau de 4 x 16 x 4 | (pour de futures clôtures) |
