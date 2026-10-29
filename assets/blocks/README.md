# Blocs

Chaque fichier `*.json` de ce dossier décrit un bloc. Son nom (sans `.json`) en est l'identifiant, celui qu'une autre fiche cite (par exemple dans `drop`). Les textures sont rangées dans `textures/`.

Pour ajouter un bloc, il suffit d'ajouter sa fiche : le rendu du terrain, les collisions, les sons, l'inventaire et le serveur lisent tout dans ces fichiers. Un fichier invalide est signalé dans la console au lancement, puis ignoré. Tous les blocs se trouvent dans le catalogue de l'inventaire (`E`), sauf ceux qui ne se ramassent pas tels quels (l'herbe, les liquides).

Les 135 blocs actuels viennent des modules de la branche `develop` (`CoreAssets`, `HeroSurvival`, `HeroCrafting`, `HeroDungeons`). Le script `tools/importer_blocs_objets.py` les reprend : il convertit les fichiers `.block`, copie les textures et tire la couleur de chaque face de la moyenne de sa texture. Le relancer après une retouche des modules remet ces fichiers à jour sans changer les numéros.

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
| `numeric_id` | Numéro du bloc dans les chunks, le réseau et les sauvegardes, de 1 à 16383 (0 est l'air). **Ne plus le changer** une fois des mondes enregistrés : un bloc posé y est noté par ce numéro. Les numéros 1 à 13 sont ceux que pose le générateur de terrain (pierre, terre, herbe, sable, neige, eau, lave, tronc, feuilles, minerais de fer et d'or, cristal, obsidienne). |
| `name`, `description` | Nom affiché et description (bulle de l'inventaire). |
| `category` | Rubrique du catalogue (`Minéraux`, `Sols`, `Flore`, `Construction`…). |
| `hardness` | Dureté, celle des modules d'origine (1 pour une fleur, 14 pour la pierre). Pas encore utilisée : tout se casse d'un clic. |
| `color` | Couleur des faces dans le terrain : `side` pour les côtés et le dessous, `top` pour le dessus (celle des côtés à défaut). `#rrggbb`, ou `#rrggbbaa` pour une face translucide. |
| `textures` | Textures PNG des faces, chemins depuis ce dossier : `side`, et au besoin `top` et `bottom`. Elles dessinent l'icône de l'inventaire ; le terrain n'utilise encore que les couleurs. |
| `texture_tint` | Teinte appliquée aux textures grises (herbe, feuillages), `#rrggbb`. |
| `shape` | `cube` (par défaut) ou `plant` : deux panneaux croisés en diagonale, pour les fleurs, les herbes, les pousses et la torche. Une plante ne cache rien derrière elle. |
| `solid` | `false` : on le traverse (plantes, torche). Vrai par défaut. Un bloc qu'on traverse se pose aussi là où se tient le joueur. |
| `opaque` | `false` : il laisse voir les faces des blocs derrière lui (verre, glace, feuillages). Vrai par défaut. |
| `liquid` | `true` : un liquide (eau, lave). Il n'est ni solide, ni opaque, ni visé par le réticule, ni ramassé ; un bloc posé le remplace. |
| `light` | Lumière émise, de 0 à 15. Pas encore rendue. |
| `sound` | Matière des pas et des blocs cassés ou posés : `stone` (par défaut), `dirt`, `grass`, `sand`, `snow`, `wood` ou `water`. |
| `drop` | Ce que donne le bloc cassé : lui-même si le champ est absent, l'identifiant d'un autre bloc (l'herbe donne `dirt`), ou `null` pour rien. |
