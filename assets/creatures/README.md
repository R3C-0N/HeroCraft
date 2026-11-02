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
| `health` | Points de vie (10 par défaut). Un coup de joueur en retire 2,5 : la bête rougit, recule, puis s'enfuit ; à 0 elle bascule sur le flanc et disparaît. |
| `min_group`, `max_group` | Taille d'un groupe qui apparaît ensemble (1 par défaut). |
| `attack` | Comportement agressif (voir plus bas). Sans ce champ, la bête est paisible : frappée, elle s'enfuit. |
| `spawn` | Les biomes où l'espèce apparaît d'elle-même (voir ci-dessous). Sans ce champ, elle n'apparaît jamais seule : c'est le cas des créatures des profondeurs, des mondes démoniaques et des bois féeriques, qui attendent leur monde. |
| `model` | Le modèle 3D (voir ci-dessous). |

Chaque entrée de `spawn` a trois champs :
- `biome` : l'un de `Plains`, `Forest`, `Desert`, `Mountains`, `SnowPeaks`, `Tundra`, `Taiga`, `Savanna`, `Jungle`, `Beach`, `Sea`, `Ocean` ou `DeepOcean` ;
- `weight` : le poids de l'espèce face aux autres espèces du même biome ;
- `chance` (1 par défaut) : la probabilité que le groupe tiré apparaisse vraiment, pour une espèce rare.

Les créatures n'apparaissent que sur l'herbe, la terre, le sable ou la neige.

## Attaque

Une bête dont la fiche a un champ `attack` chasse les joueurs et ses proies qu'elle voit, les poursuit et les frappe. Frappée par un joueur, elle riposte au lieu de fuir. Exemple, le loup :

```json
"attack": {"mode": "pack", "damage": 4, "cooldown": 1.0, "sight": 14, "chase_speed": 1.6, "pack_size": 3, "prey": ["lapin", "cerf"]}
```

| Champ | Rôle |
|---|---|
| `mode` | `solo` : elle poursuit sa cible et la frappe au contact. `pack` : elle n'ose attaquer qu'entourée d'au moins `pack_size` bêtes de son espèce (à moins de 16 blocs), qui se joignent à la chasse et encerclent la cible ; frapper l'une ameute les autres ; trop peu nombreuses, elles fuient. `charge` : elle s'arrête, vise, fonce en ligne droite, puis souffle un moment. |
| `damage` | Points de vie retirés par coup, au joueur ou à la proie. |
| `cooldown` | Secondes entre deux coups (1,2 par défaut). |
| `sight` | Distance à laquelle elle repère une cible, en blocs (12 par défaut). Une cible prise en chasse n'est lâchée qu'au-delà de 1,6 fois cette distance. |
| `reach` | Allonge d'un coup devant son museau, en blocs (0,8 par défaut). |
| `chase_speed` | Allure de la poursuite ou de la charge, par rapport à `walk_speed` (1,6 par défaut). |
| `hunts_players` | Elle s'en prend aux joueurs qu'elle voit (`true` par défaut). |
| `provoked_only` | Elle n'attaque qu'une fois frappée, et reste en colère 20 secondes (`false` par défaut). |
| `prey` | Espèces qu'elle chasse (identifiants de fichiers). Après avoir tué une proie, elle les laisse tranquilles une minute. |
| `pack_size` | En meute, nombre de bêtes qu'il lui faut, elle comprise (3 par défaut). |

Le bestiaire ne décrit pas d'attaque : `tools/importer_bestiaire.py` garde celle de la fiche existante.

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
