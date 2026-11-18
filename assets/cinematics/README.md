# Cinématiques

Chaque fichier `*.json` de ce dossier décrit une cinématique jouée **dans le moteur** : la caméra suit une suite de plans autour du héros, l'écran se couvre de bandeaux noirs, de textes et de fondus, et la cinématique peut renvoyer des événements au jeu (changer d'époque, retirer un objet…). Son nom (sans `.json`) en est l'identifiant.

Pendant une cinématique, le joueur ne bouge plus, le HUD, le réticule et le chat disparaissent, et les écrans ouverts (inventaire, carte, roue…) se referment. **Échap**, **Espace** ou **Entrée** proposent de passer ; un second appui dans les 3 secondes passe pour de bon. Une cinématique passée renvoie quand même ses événements au jeu, pour que la partie reste cohérente.

Le ton et le minutage s'inspirent des maquettes HTML de `docs/cinematiques/` (la Genèse, la Chronique du Roi), qui restent des films à part, hors du moteur.

## Essayer

Dans le chat :
- `/cinematic` liste les cinématiques, `/cinematic fin-de-la-vision` en joue une (Tab complète le nom), même déjà vue.
- `/event stone_obtained stone=voidStone kind=whole` envoie l'événement « Pierre du Néant obtenue » aux pieds du joueur, comme le ferait le jeu : les cinématiques qu'il déclenche se jouent.

Pour voir l'écran des cinématiques sans lancer le jeu (textes, bandeaux et fondus, sans la 3D) :
`HEROCRAFT_CAPTURES=captures cargo test -p herocraft_client snapshot_cinematics -- --ignored`, avec un fond au choix (`HEROCRAFT_CAPTURES_FOND=image.png`).

## Exemple

```json
{
  "title": "La fin de la vision",
  "triggers": [{"event": "stone_obtained", "when": {"stone": "voidStone"}}],
  "shots": [
    {"name": "Gros plan", "duration": 5, "position": [[0.7, 1.75, 2.6], [0.35, 1.6, 1.5]], "look_at": [0, 1.4, 0], "fov": [45, 36]},
    {"name": "Tour du héros", "duration": 6, "orbit": {"radius": 3, "height": 1, "angle": [200, 330]}, "shake": 0.03}
  ],
  "captions": [{"at": 1, "duration": 4, "text": "La Pierre pulse entre tes mains.", "style": "narration"}],
  "fades": [{"at": 9, "duration": 2, "from": 0, "to": 1, "color": "#FFFFFF"}],
  "cues": [{"at": 11, "emit": "vision_ended"}],
  "next": "reveil-au-present"
}
```

## Repère

Les positions sont en **blocs**, dans le repère de l'**ancre**, figé au début de la cinématique : `x` vers la droite, `y` vers le haut, `z` vers l'avant (là où regarde le joueur). `[0, 1.6, 0]` est donc à hauteur des yeux d'un héros debout sur l'ancre, `[0, 2, -4]` quatre blocs derrière lui.

La caméra ne descend jamais sous le relief : une position sous le sol est remontée juste au-dessus.

## Champs de la cinématique

| Champ | Rôle |
|---|---|
| `title` | Nom de la cinématique (journal du jeu). |
| `description` | Ce qu'elle montre, pour s'y retrouver. |
| `anchor` | `player` (par défaut) : les pieds et le regard du joueur. `event` : la position de l'événement déclencheur (une Pierre, une porte), tournée comme le regard du joueur ; le joueur à défaut. |
| `triggers` | Événements du jeu qui la lancent (voir plus bas). Sans déclencheur, elle ne se joue que par `/cinematic` ou par `next`. |
| `once` | `true` (par défaut) : ne se déclenche qu'une fois par monde. Les cinématiques vues sont enregistrées dans `cinematiques.json`, dans la sauvegarde du monde. |
| `skippable` | `true` (par défaut) : le joueur peut la passer. |
| `letterbox` | `true` (par défaut) : bandeaux noirs en haut et en bas, qui glissent à l'entrée et à la sortie. |
| `shots` | Les plans, joués l'un après l'autre (au moins un). |
| `captions` | Les textes. |
| `fades` | Les fondus. |
| `cues` | Événements renvoyés au jeu à un instant donné. |
| `on_end` | Noms d'événements renvoyés au jeu à la fin, qu'elle soit allée au bout ou ait été passée. |
| `next` | Cinématique enchaînée à la fin (son identifiant). |

La durée totale est celle du dernier plan, texte, fondu ou événement qui se termine. Si un texte ou un fondu dure après le dernier plan, la caméra reste où ce plan l'a laissée.

## Plans (`shots`)

Une paire `[début, fin]` est parcourue pendant le plan ; une valeur seule reste constante. Pour une position, la paire s'écrit `[[x, y, z], [x, y, z]]`.

| Champ | Rôle |
|---|---|
| `name` | Nom du plan, pour s'y retrouver. |
| `duration` | Durée en secondes. |
| `position` | Position de la caméra (`[x, y, z]`, ou une paire pour un travelling). |
| `orbit` | Ou bien une caméra qui tourne autour du point visé : `radius` (distance en blocs), `height` (au-dessus du point visé, 0 par défaut) et `angle` en degrés, compté depuis l'arrière de l'ancre (0 : derrière, 90 : à droite, 180 : face à elle). Chacun peut être une paire. |
| `look_at` | Point visé, `[0, 1.6, 0]` par défaut (la tête). Une paire fait glisser le regard. |
| `fov` | Champ de vision vertical en degrés, 50 par défaut. Une paire fait un zoom. |
| `roll` | Inclinaison de la caméra en degrés (0 par défaut). |
| `shake` | Tremblement en blocs (0 par défaut) : 0,02 pour une vibration, 0,1 pour un séisme. |
| `ease` | Courbe d'avancement : `in_out` (par défaut, démarre et s'arrête doucement), `in`, `out` ou `linear`. |

Chaque plan a soit `position`, soit `orbit`.

## Textes (`captions`)

| Champ | Rôle |
|---|---|
| `at`, `duration` | Début (en secondes depuis le début de la cinématique) et durée. |
| `text` | Le texte. Il revient à la ligne de lui-même. |
| `style` | `subtitle` (par défaut) : sous-titre en bas, au-dessus du bandeau. `narration` : voix du conteur, en italique sous le centre. `title` : grand titre au centre, police des titres, doré. `place` : lieu ou époque, discret en haut à gauche. |
| `speaker` | Qui parle, en doré devant un sous-titre (« Le Roi : … »). |
| `fade` | Secondes d'apparition et de disparition en fondu (0,5 par défaut). |

Les textes sont clairs sur fond sombre ; sur un voile clair (fondu au blanc), ils passent d'eux-mêmes à l'encre sombre.

## Fondus (`fades`)

| Champ | Rôle |
|---|---|
| `at`, `duration` | Début et durée. |
| `from`, `to` | Opacité du voile au début et à la fin : 0 pour l'image nette, 1 pour la couleur pleine. |
| `color` | `#RRGGBB`, noir par défaut. |

Le dernier fondu commencé décide du voile ; une fois fini, il reste à son opacité de fin. Pour ouvrir sur du noir : `{"at": 0, "duration": 2, "from": 1, "to": 0}`.

## Événements

### Ce qui déclenche une cinématique (`triggers`)

Le jeu envoie des événements au lecteur (`Cinematics::notify` dans le client, type `herocraft_common::cinematics::GameEvent`) : un nom, des étiquettes `clé = valeur`, et une position facultative. Un déclencheur nomme l'événement et, dans `when`, les étiquettes qu'il doit porter (toutes) :

```json
"triggers": [{"event": "stone_obtained", "when": {"stone": "voidStone", "kind": "whole"}}]
```

| Événement | Envoyé par | Étiquettes |
|---|---|---|
| `stone_obtained` | les Pierres du Néant, quand le joueur en obtient une | `stone` (identifiant de l'objet : `voidStone`, `voidStoneFragment`), `kind` (`whole` ou `fragment`) ; position de la Pierre |

### Ce que la cinématique renvoie au jeu (`cues`, `on_end`)

```json
"cues": [{"at": 16.6, "emit": "vision_ended", "tags": {"stone": "voidStone"}}]
```

Le jeu reçoit ces événements comme ceux qu'il envoie : ils peuvent aussi déclencher d'autres cinématiques. À la fin de chacune, le lecteur renvoie en plus `cinematic_ended` (étiquettes `id`, et `skipped` = `true` si elle a été passée).

| Événement | Renvoyé par | Rôle attendu |
|---|---|---|
| `vision_ended` | `fin-de-la-vision`, au plus blanc du fondu | Fin de la vision tutoriel : le jeu renvoie le joueur au présent (ruines du temple) et détruit la Pierre absorbée. |
| `awakened` | `reveil-au-present`, à la fin | Le joueur a repris la main au présent. |

## Les cinématiques du jeu

| Fichier | Quand | Ce qu'elle montre |
|---|---|---|
| `fin-de-la-vision` | Pierre du Néant entière obtenue | La Pierre s'éveille, des souvenirs qui ne sont pas ceux du joueur, puis la lumière efface tout (fondu au blanc, `vision_ended`). Enchaîne sur `reveil-au-present`. (19,5 s) |
| `reveil-au-present` | Après la fin de la vision | Le joueur s'éveille dans les ruines du temple, sans autre souvenir que la vision ; la caméra descend du ciel et se pose derrière lui. (15 s) |

Les textes sont une première version, à reprendre avec l'écriture du scénario.
