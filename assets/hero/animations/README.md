# Gestes du héros

Chaque fichier `*.json` de ce dossier décrit un geste du héros joué par une compétence : un coup, une incantation, un tir, un cri… Son nom (sans `.json`) en est l'identifiant, celui que cite le champ `animation` d'une compétence. Une douzaine de gestes servent aux 180 compétences de la roue.

Un geste est une courte suite de **poses clés** : à chaque clé, quelques os du héros tournent (et le corps entier peut se déplacer) ; entre deux clés, la pose est interpolée. Un os qu'une clé ne cite pas revient à sa pose de repos, si bien qu'une clé sans `bones` est la pose de repos tout entière.

Le client seul lit ces fichiers. Le lecteur de gestes n'est pas encore écrit (il attend les articulations du héros) ; `tools/verifier_effets_gestes.py` vérifie déjà les fichiers.

## Exemple

```json
{
  "description": "Arme levée à deux mains puis abattue au sol, genoux fléchis.",
  "duration": 0.9,
  "legs": "locked",
  "impact": 0.35,
  "keys": [
    {"t": 0.0},
    {"t": 0.25, "ease": "out", "bones": {
      "rightArm": {"rotation": [165, 0, 10]}, "leftArm": {"rotation": [160, 0, 10]},
      "torso": {"rotation": [-12, 0, 0]}}},
    {"t": 0.35, "ease": "in", "bones": {
      "rightArm": {"rotation": [50, 0, 5]}, "leftArm": {"rotation": [50, 0, 5]},
      "torso": {"rotation": [30, 0, 0]}, "rightShin": {"rotation": [60, 0, 0]},
      "body": {"translation": [0, -0.25, 0]}}},
    {"t": 1.0}
  ]
}
```

## Champs

| Champ | Rôle |
|---|---|
| `description` | Ce que fait le geste. |
| `duration` | Durée du geste entier, en secondes. Une compétence peut l'accélérer ou le ralentir. |
| `legs` | `free` : les jambes continuent de marcher sous le geste, on peut frapper en avançant ; les os des jambes cités par une clé l'emportent sur la marche. `locked` : la marche s'arrête le temps du geste (incantation, frappe au sol, bond). |
| `impact` | Instant (de 0 à 1) où le coup porte, si la compétence n'en donne pas un autre. |
| `loop` | Intervalle `[début, fin]` (de 0 à 1) rejoué tant qu'une canalisation ou une garde dure ; le geste finit ensuite normalement. |
| `keys` | Les poses clés, par `t` croissant, de 0 à 1. |

Chaque clé :
- `t` : instant de la clé, de 0 (début) à 1 (fin) ;
- `ease` : comment on arrive à cette clé depuis la précédente : `inOut` (par défaut, en douceur), `in` (accélère, pour un coup qui s'abat), `out` (ralentit, pour une prise d'élan), `linear` (vitesse constante, pour un tour sur soi) ;
- `bones` : les os qui quittent leur pose de repos, chacun avec `rotation` et, pour `body` seulement, `translation`.

## Os

| Os | Pivot | Porte |
|---|---|---|
| `body` | les pieds | le héros entier : se baisser, pencher, tourner sur soi |
| `torso` | les hanches | le buste, la tête et les bras ; le tourner fait pivoter le haut du corps jambes plantées |
| `head` | le cou | la tête et ce qui y est accroché |
| `rightArm`, `leftArm` | l'épaule | le bras entier |
| `rightForearm`, `leftForearm` | le coude | l'avant-bras et la main |
| `rightHand`, `leftHand` | le poignet | la main et l'objet tenu (arme, bouclier, arc) |
| `rightLeg`, `leftLeg` | la hanche | la jambe entière |
| `rightShin`, `leftShin` | le genou | le bas de la jambe |

Les avant-bras, les mains et les tibias arrivent avec les articulations du héros ; avant elles, ces os ne bougeront rien.

## Sens des rotations

`rotation` vaut `[x, y, z]` en degrés, autour du pivot de l'os :

| Axe | Valeur positive |
|---|---|
| `x` | bras ou jambe levé vers l'avant ; torse ou tête penchés en avant ; avant-bras plié vers le haut ; genou plié ; poignet qui incline l'arme pointe en avant |
| `y` | tourne vers la gauche du héros |
| `z` | bras ou jambe écarté vers l'extérieur (même sens à gauche et à droite) ; torse ou tête penchés vers la gauche |

Repères : un bras tendu à l'horizontale devant soi vaut `[90, 0, 0]`, levé au-dessus de la tête `[170, 0, 0]`, ouvert sur le côté `[0, 0, 90]`. Un accroupi plie les hanches (`rightLeg` à 30) et les genoux (`rightShin` à 60) et baisse le corps (`body` en `translation` `[0, -0.25, 0]`).

`translation` (sur `body` seulement) vaut `[gauche, haut, avant]` en blocs. Elle ne déplace que l'image : un vrai déplacement (ruée, bond) vient de la compétence.

## Liste des gestes

| Geste | Mouvement | Durée | Impact |
|---|---|---|---|
| `overheadStrike` | coup de haut en bas | 0,7 s | 0,45 |
| `sweep` | balayage horizontal | 0,6 s | 0,5 |
| `thrust` | estoc vers l'avant | 0,5 s | 0,45 |
| `groundSlam` | arme abattue au sol | 0,9 s | 0,35 |
| `spin` | tourbillon sur soi | 0,8 s | 0,5 |
| `charge` | ruée, épaule en avant | 0,6 s | 0,5 |
| `guard` | bouclier levé, tenu en boucle | 0,6 s | — |
| `channel` | mains en avant, en boucle | 1,2 s | 0,3 |
| `bless` | main tendue, paume ouverte | 0,7 s | 0,5 |
| `throw` | bras armé puis lancer | 0,7 s | 0,55 |
| `bowShot` | bander et tirer | 1,0 s | 0,7 |
| `crossbowShot` | épauler et tirer | 0,8 s | 0,5 |
| `shout` | torse bombé, bras écartés | 0,9 s | 0,4 |
| `summon` | bras levés au ciel, en boucle | 1,2 s | 0,6 |
| `leap` | accroupi, détente, réception | 0,7 s | 0,2 |
