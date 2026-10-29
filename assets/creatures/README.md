# Créatures

Chaque fichier `*.json` de ce dossier décrit une espèce. Son nom (sans `.json`) en est l'identifiant, celui que le serveur envoie aux clients. Le modèle 3D (glTF) et sa texture sont rangés dans `modeles/`.

Pour ajouter une bête, il suffit d'ajouter une description et son modèle : la simulation et le rendu n'ont aucun code propre à une espèce. Un fichier invalide est signalé dans la console au lancement, puis ignoré. Une espèce dont le modèle est illisible est dessinée en pavé gris.

Les 19 créatures actuelles viennent du module `HeroBestiary` de la branche `develop`. Le script `tools/importer_bestiaire.py` les reprend : il convertit les prefabs et copie les modèles et les textures. Le relancer après une retouche du bestiaire remet ces fichiers à jour.

## Exemple

```json
{
  "name": "Loup",
  "description": "Chasse en meute à la lisière des forêts.",
  "body": {"half_width": 0.225, "half_length": 0.8614, "height": 1.0125},
  "walk_speed": 1.7,
  "min_group": 3,
  "max_group": 4,
  "spawn": [
    {"biome": "Forest", "weight": 0.6},
    {"biome": "Taiga", "weight": 0.6, "chance": 0.5}
  ],
  "model": {"mesh": "modeles/loup.gltf", "texture": "modeles/loup.png", "walk_animation": "repos"}
}
```

## Champs

| Champ | Rôle |
|---|---|
| `name`, `description` | Nom affiché et description. |
| `body` | Volume utilisé pour les déplacements, en blocs : `half_width` (demi-largeur), `half_length` (demi-longueur, de la tête à la queue) et `height`. La créature ne passe que là où ce corps tient. |
| `walk_speed` | Vitesse de promenade, en blocs par seconde. À 0, la créature ne bouge pas. |
| `min_group`, `max_group` | Taille d'un groupe qui apparaît ensemble (1 par défaut). |
| `spawn` | Les biomes où l'espèce apparaît d'elle-même (voir ci-dessous). Sans ce champ, elle n'apparaît jamais seule : c'est le cas des créatures des profondeurs, des mondes démoniaques et des bois féeriques, qui attendent leur monde. |
| `model` | Le modèle 3D (voir ci-dessous). |

Chaque entrée de `spawn` a trois champs :
- `biome` : l'un de `Plains`, `Forest`, `Desert`, `Mountains`, `SnowPeaks`, `Tundra`, `Taiga`, `Savanna`, `Jungle`, `Beach`, `Sea`, `Ocean` ou `DeepOcean` ;
- `weight` : le poids de l'espèce face aux autres espèces du même biome ;
- `chance` (1 par défaut) : la probabilité que le groupe tiré apparaisse vraiment, pour une espèce rare.

Les créatures n'apparaissent que sur l'herbe, la terre, le sable ou la neige.

## Modèle

| Champ | Rôle |
|---|---|
| `mesh` | Fichier `.gltf` (glTF 2.0, données embarquées en base64) : un maillage à squelette, avec positions, normales, coordonnées de texture, os et poids. |
| `texture` | Texture PNG, lue au texel près (sans lissage). Un texel transparent (alpha sous 0,5) découpe la pièce. |
| `walk_animation` | Animation du glTF jouée pendant la marche (`repos` par défaut, comme dans le bestiaire). Elle tourne d'autant plus vite que la bête avance vite, et se fige quand elle s'arrête. |

Le modèle est exprimé en blocs, avec y vers le haut et la tête vers +z. Le point le plus bas de sa pose de repos est posé sur les pieds de la créature. Les modèles exportés en glTF par Blockbench conviennent, à condition d'embarquer leurs données.

Pour voir les modèles sans lancer le jeu :

```bash
APERCU=apercu.png cargo test -p herocraft_client apercu -- --ignored
```
