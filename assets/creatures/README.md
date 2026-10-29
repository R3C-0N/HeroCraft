# Créatures

Chaque fichier `*.json` de ce dossier décrit une espèce. Son nom (sans `.json`) en est l'identifiant, celui que le serveur envoie aux clients. Pour ajouter une bête, il suffit d'ajouter un fichier : la simulation et le rendu n'ont pas de code propre à chaque espèce. Un fichier invalide est signalé dans la console au lancement, puis ignoré.

Le serveur et les clients lisent chacun leur propre dossier. Un client qui ne connaît pas une espèce envoyée par le serveur la dessine en pavé gris.

## Exemple

```json
{
  "name": "Mouton",
  "body": {"half_width": 0.35, "half_length": 0.55, "height": 1.2},
  "walk_speed": 1.0,
  "max_group": 4,
  "spawn": [
    {"biome": "Plains", "weight": 5},
    {"biome": "Desert", "weight": 1, "chance": 0.3}
  ],
  "model": {
    "parts": [
      {"name": "corps", "from": [-5, 8, -8], "to": [5, 17, 8], "color": "#d6d0c4"},
      {"name": "patte avant gauche", "from": [1.5, 0, 3.5], "to": [4.5, 8, 6.5], "color": "#3b302a", "swing": {"amplitude": 30, "phase": 0}}
    ]
  }
}
```

## Champs

| Champ | Rôle |
|---|---|
| `name` | Nom affiché. |
| `body` | Volume utilisé pour les déplacements, en blocs : `half_width` (demi-largeur), `half_length` (demi-longueur, de la tête à la queue) et `height`. La créature ne passe que là où ce corps tient : elle évite les murs et les plafonds trop bas. |
| `walk_speed` | Vitesse de marche, en blocs par seconde. |
| `max_group` | Taille maximale d'un groupe qui apparaît ensemble. Vaut 1 par défaut. |
| `spawn` | Les biomes où l'espèce apparaît d'elle-même. `weight` est son poids face aux autres espèces du même biome. `chance` (1 par défaut) est la probabilité que le groupe tiré apparaisse vraiment, pour une espèce rare. Les biomes possibles sont `Plains`, `Forest`, `Desert`, `Mountains`, `SnowPeaks`, `Tundra`, `Taiga`, `Savanna`, `Jungle`, `Beach`, `Sea`, `Ocean` et `DeepOcean`. Les créatures n'apparaissent que sur l'herbe, la terre, le sable ou la neige. |
| `model` | Le modèle 3D, fait de pavés colorés (voir ci-dessous). |

## Modèle

Le repère du modèle a son origine sous les pieds, au centre de la créature. **x** va vers sa gauche, **y** vers le haut et **z** vers l'avant, du côté de la tête. Les mesures sont exprimées en unités, avec `units_per_block` unités par bloc (16 par défaut, comme dans Blockbench).

Champs de chaque pièce (`parts`) :

| Champ | Rôle |
|---|---|
| `name` | Nom de la pièce, pour s'y retrouver dans le fichier. |
| `from`, `to` | Deux coins opposés du pavé. |
| `color` | Couleur `#rrggbb`. Les pièces sont éclairées et ombrées comme le relief. |
| `swing` | Balancement pendant la marche : rotation autour de l'axe gauche-droite. `amplitude` est l'angle maximal en degrés. `phase` est le décalage dans le cycle du pas (0.5 : à contretemps). Le balancement s'arrête quand la créature s'arrête. |
| `pivot` | Point autour duquel la pièce se balance. Par défaut, c'est le centre de son dessus, ce qui convient à une patte. Les pièces qui doivent bouger ensemble (une tête, ses oreilles, son groin) prennent le même `pivot` et le même `swing`. |

Pour faire marcher une bête, on donne la même `phase` aux pattes en diagonale, par exemple 0 pour l'avant gauche et l'arrière droite, et 0.5 pour les deux autres.
