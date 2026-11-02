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
  "model": {
    "mesh": "modeles/loup.gltf",
    "texture": "modeles/loup.png",
    "animations": {
      "marche": {"foulee": 0.56, "os": {
        "patteAVD": {"rotation": [22, 0, 0]},
        "patteAVG": {"rotation": [22, 0, 0], "phase": 0.5}
      }},
      "sommeil": {"cycle": 4, "pause": {"poids": 0.4, "duree": [12, 25]}, "os": {
        "racine": {"translation_fixe": [0, -0.37, 0]}
      }}
    }
  }
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
| `animations` | Les animations de la bête, par nom (voir ci-dessous). Sans ce champ, l'animation `repos` du glTF est jouée pendant la marche, comme dans le bestiaire. |

Le modèle est exprimé en blocs, avec y vers le haut et la tête vers +z. Le point le plus bas de sa pose de repos est posé sur les pieds de la créature. Les modèles exportés en glTF par Blockbench conviennent, à condition d'embarquer leurs données.

## Animations

Chaque animation est jouée en boucle. Le jeu choisit laquelle selon ce que fait la bête :

| Nom | Quand |
|---|---|
| `repos` | Arrêtée. C'est aussi l'animation de repli quand celle demandée manque. |
| `marche` | Elle se promène. |
| `course` | Elle fuit (ou poursuit une proie). Sans `course`, elle marche plus vite. |
| `attaque` | Elle attaque. Sans `attaque`, elle reste au repos. |
| `coup` | Jouée une fois par-dessus le reste quand un joueur la frappe, le temps qu'elle rougisse : un cycle dure ce coup. Une `frequence` de 0,5 fait un seul sursaut qui revient à zéro. |
| les autres | Ses inactivités (`broute`, `renifle`, `sommeil`, `hurle`…) : celles qui ont une `pause`. |

Quand une bête s'arrête, le serveur tire au poids l'une de ses inactivités (celles qui ont une `pause`, `repos` compris s'il en a une) et la lui fait tenir le temps indiqué. Tous les joueurs la voient donc dormir ou brouter en même temps. On passe d'une animation à la suivante en fondu (0,3 seconde).

| Champ | Rôle |
|---|---|
| `cycle` | Durée d'un cycle, en secondes (1 par défaut). |
| `foulee` | Distance parcourue en un cycle, en blocs. Le cycle suit alors le chemin parcouru, et pas le temps : les pattes ne glissent pas, quelle que soit la vitesse. Pour `marche` et `course`. |
| `pause` | Fait de l'animation une inactivité : `poids` face aux autres, et `duree` en secondes (`[min, max]`). |
| `clip` | Nom d'une animation du glTF jouée sous les mouvements d'os (une animation faite dans Blockbench, par exemple). |
| `os` | Le mouvement de chaque os, par nom de nœud du glTF. Un nom qui finit par `*` vise tous les os qui commencent ainsi (`queue*` : `queue1`, `queue2`, `queue3`). Les noms en double (les pattes du mille-pattes) sont tous visés. |

Chaque os prend une pose fixe et une oscillation en sinus, ajoutées à sa pose de repos :

| Champ | Rôle |
|---|---|
| `rotation_fixe` | Angles tenus pendant toute l'animation, en degrés `[x, y, z]`. |
| `rotation` | Amplitude de l'oscillation des angles, en degrés. |
| `translation_fixe` | Déplacement tenu, en blocs. Sur `racine`, il descend tout le corps (`sommeil`). |
| `translation` | Amplitude de l'oscillation du déplacement, en blocs. |
| `phase` | Avance de l'oscillation, en fraction de cycle : 0,5 la met à contretemps (pattes opposées). |
| `frequence` | Oscillations par cycle (1 par défaut) : 2 pour une tête qui hoche deux fois par pas. |
| `decalage` | Avance de phase ajoutée à chaque os suivant que vise le même nom : une onde le long d'une queue ou des pattes d'un mille-pattes. |

Les axes sont ceux du modèle, autour du point d'attache de l'os :
- x positif : bascule en avant. La tête pique du nez, le bas d'une patte part vers l'arrière, une queue tendue vers l'arrière se relève, une oreille dressée part vers l'avant.
- y positif : tourne vers la gauche de la bête.
- z positif : penche sur sa droite (le côté gauche monte).

Les os de chaque modèle sont nommés dans son glTF : `racine` (tout le corps), `tete`, `patteAVD`, `patteAVG`, `patteARD`, `patteARG` (avant ou arrière, droite ou gauche), `queue`… Un os que le modèle n'a pas est signalé dans la console au lancement, puis ignoré.

Pour voir les modèles sans lancer le jeu :

```bash
APERCU=apercu.png cargo test -p herocraft_client apercu_creatures -- --ignored
```

Et pour voir toutes les animations, une ligne par bête et une colonne par animation (rangées par nom), au quart de leur cycle :

```bash
APERCU=animations.png APERCU_ESPECES=loup,ours APERCU_PHASE=0.25 APERCU_CASE=300 cargo test -p herocraft_client apercu_animations -- --ignored
```
