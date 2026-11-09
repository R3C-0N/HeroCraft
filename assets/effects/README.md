# Effets visuels

Chaque fichier `*.json` de ce dossier décrit un effet visuel joué par une compétence : une onde au sol, une traînée de coup, une boule de feu, des paillettes de soin… Son nom (sans `.json`) en est l'identifiant, celui que cite le champ `visuals` d'une compétence. Les textures des particules et des formes sont rangées dans `textures/` : des images en niveaux de blanc, teintées par la couleur de la couche.

Un effet ne dit que **à quoi il ressemble**. **Où** et **quand** il se joue (main droite, pieds, cible touchée, projectile ; au début du geste, à l'impact…) est écrit dans la compétence qui l'utilise, si bien qu'un même effet sert à plusieurs compétences.

Le client seul lit ces fichiers : le serveur n'en a pas besoin. Le moteur qui les joue n'est pas encore écrit ; `tools/verifier_effets_gestes.py` vérifie déjà les fichiers.

## Exemple

```json
{
  "description": "Onde lumineuse qui court au sol jusqu'au bord de la zone.",
  "unit": "area",
  "layers": [
    {"shape": "ring", "on_ground": true, "duration": 0.45,
     "radius": [0.2, 1.0], "thickness": 0.3,
     "color": ["#FFE9A8", "#FFB54700"], "glow": 3},
    {"particles": {"count": 40, "origin": "ring", "origin_radius": 1.0, "direction": "outward",
     "speed": [1, 2.5], "gravity": -2, "life": [0.4, 0.8],
     "size": [0.08, 0.02], "color": ["#FFF2C0", "#FFB54700"], "glow": 2, "texture": "spark.png"},
     "delay": 0.3}
  ]
}
```

## Champs de l'effet

| Champ | Rôle |
|---|---|
| `description` | Ce que l'effet montre, pour s'y retrouver. |
| `unit` | `blocks` (par défaut) : les longueurs sont en blocs. `area` : elles sont des fractions de la taille de la zone de la compétence (son rayon, sa portée ou sa longueur) ; `radius: 1.0` finit alors exactement au bord de la zone, quel que soit le rang. |
| `layers` | Les couches, dessinées ensemble. Chacune est soit une **forme** (`shape`), soit des **particules** (`particles`). |

## Temps d'une couche

Ces champs se placent au niveau de la couche, à côté de `shape` ou de `particles`.

| Champ | Rôle |
|---|---|
| `delay` | Secondes avant que la couche commence (0 par défaut). |
| `duration` | Durée de la couche, en secondes. Les valeurs « au fil de la vie » vont de leur début à leur fin sur cette durée. |
| `loop` | `true` : la couche dure tant que le visuel dure (de `from` à `to` dans la compétence : la lueur d'une arme jusqu'à l'impact, le givre tant que la cible est ralentie). Une forme garde alors sa valeur de début ; des particules sont émises en continu. |
| `fade_in`, `fade_out` | Secondes d'apparition et de disparition en fondu, pour une couche en boucle. |

## Valeurs « au fil de la vie » et valeurs tirées au hasard

Une paire `[a, b]` se lit de deux façons selon le champ :
- **au fil de la vie** (`radius`, `thickness`, `size`, `length`, `width`, `color`) : la valeur passe de `a` à `b` pendant la vie de la couche ou de la particule ;
- **tirée au hasard** (`speed`, `life`, `spin` des particules) : chaque particule prend une valeur entre `a` et `b`.

Une valeur seule est constante. Une couleur s'écrit `#RRGGBB` ou `#RRGGBBAA` (l'opacité à la fin) : `["#FFE9A8", "#FFB54700"]` passe du jaune pâle à l'orange en s'effaçant.

`glow` multiplie la couleur. Au-dessus de 1, elle déborde dans le bloom du rendu HDR et luit : 1,5 pour une lueur douce, 3 à 5 pour le feu et la lumière sacrée. Sans `glow` (ou à 1), la couche ne brille pas : terre, fumée, rocher.

## Formes (`shape`)

| Forme | Paramètres | Exemple |
|---|---|---|
| `ring` | `radius`, `thickness`, `on_ground` ou `upright` | onde de choc, bord d'un dôme |
| `disc` | `radius`, `spin` (degrés par seconde), `texture`, `on_ground` ou `upright` | sceau au sol, portail |
| `arc` | `radius`, `angle` (degrés), `thickness`, `sweep` | traînée d'un coup de taille |
| `streak` | `length`, `width` | trait d'un estoc, droit devant |
| `dome` | `radius`, `pulse` | bouclier |
| `column` | `radius`, `height` | colonne de lumière |
| `sprite` | `size`, `texture`, `spin` | cœur d'une boule de feu, éclair d'un impact |
| `trail` | `width`, `life` (secondes de traînée gardées) | sillage d'un projectile |

- `on_ground` : la forme est couchée sur le sol sous l'ancre. `upright` : elle est debout, face à la direction visée. Sinon elle est à plat dans le plan du regard (arc, traînée) ou face à la caméra (sprite).
- `sweep` (arc) : l'arc se dessine progressivement dans le sens du coup au lieu d'apparaître d'un bloc.
- `pulse` : la forme bat doucement à cette fréquence (en hertz).

## Particules (`particles`)

| Champ | Rôle |
|---|---|
| `count` | Nombre de particules lâchées d'un coup au début de la couche. |
| `rate` | Ou bien : particules par seconde, émises en continu (avec `loop` ou `duration`). |
| `origin` | D'où elles partent : `point`, `sphere`, `ring`, `disc` (au sol), `cylinder` (le long du corps), `line` (le long de l'arme). Un `ring` ou un `disc` d'origine est couché au sol, sauf avec `"upright": true` (debout face à la visée, comme un portail). |
| `origin_radius`, `origin_height`, `origin_length` | Taille de cette origine. |
| `direction` | `up`, `outward` (en s'éloignant du centre), `inward`, `forward` (vers la visée), `random`, ou `none` (immobiles). |
| `spread` | Ouverture du cône de départ autour de la direction, en degrés. |
| `speed`, `life` | Vitesse de départ (blocs par seconde) et durée de vie (secondes), tirées au hasard. |
| `gravity` | Accélération vers le bas, en blocs par seconde² ; négative, les particules montent (braises, fumée). |
| `drag` | Freinage : plus il est fort, plus vite elles s'arrêtent. |
| `size`, `color`, `glow`, `texture` | Taille (blocs) et couleur au fil de la vie, lueur, image. |
| `spin` | Rotation de chaque particule, en degrés par seconde, tirée au hasard. |
| `orbit` | Les particules tournent autour de l'origine à cette vitesse (degrés par seconde) : étoiles d'étourdissement. |
| `offset` | Décalage `[gauche, haut, avant]` en blocs par rapport à l'ancre. |
| `block_color` | `true` : la couleur est prise au bloc sous l'ancre (poussière d'herbe, de sable ou de neige) et `color` n'en garde que l'opacité. |

## Textures (`textures/`)

| Fichier | Allure |
|---|---|
| `soft.png` | tache ronde et floue : lueurs, cœurs de sorts |
| `spark.png` | point brillant à quatre rayons : étincelles |
| `flame.png` | goutte de flamme |
| `smoke.png` | volute de fumée |
| `shard.png` | éclat en losange : terre, roche, glace |
| `star.png` | étoile à cinq branches : soin, étourdissement |
| `rune.png` | cercle de runes : sceaux, portails |

## Liste des effets

| Effet | Allure |
|---|---|
| `weaponGlow` | arme qui s'illumine |
| `slashArc` | traînée d'un coup de taille |
| `thrustStreak` | trait d'un estoc |
| `impactSpark` | étincelles au contact |
| `shockRing` | onde au sol qui s'étend |
| `groundBurst` | éclats de terre |
| `dustPuff` | poussière de ruée ou de réception |
| `shieldDome` | dôme protecteur |
| `auraRing` | cercle lumineux persistant |
| `healSparkles` | paillettes de soin montantes |
| `holyBeam` | colonne de lumière |
| `runeCircle` | sceau magique au sol |
| `handFlame` | flamme dans la main |
| `fireOrb` | boule de feu en vol |
| `fireExplosion` | explosion de feu |
| `slowFrost` | givre sur la cible |
| `rockShard` | rocher projeté |
| `windBlades` | lames de vent |
| `poisonCloud` | nuage toxique |
| `shadowSmoke` | fumée d'ombre |
| `summonPortal` | portail d'invocation |
| `stunStars` | étoiles d'étourdissement |
| `arrowTrail` | traînée de flèche |
