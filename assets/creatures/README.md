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
  "temperament": "indifferent",
  "spawn": [
    {"biome": "Forest", "weight": 0.6},
    {"biome": "Taiga", "weight": 0.6, "chance": 0.5}
  ],
  "activity": "nocturnal",
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
| `tags` | Familles de l'espèce, que certaines compétences visent : `shadow` pour les créatures de l'ombre (l'Arme de l'Aube les frappe plus fort). |
| `armor` | Armure (0 par défaut) : réduit les coups physiques, dégâts × 100 / (100 + armure). À 50, un tiers de moins ; à 100, la moitié. Certaines compétences la percent ou la brisent. |
| `resist` | Résistance (0 par défaut) : réduit de la même façon les coups magiques. |
| `body` | Volume utilisé pour les déplacements, en blocs : `half_width` (demi-largeur), `half_length` (demi-longueur, de la tête à la queue) et `height`. La créature ne passe que là où ce corps tient. |
| `walk_speed` | Vitesse de promenade, en blocs par seconde. À 0, la créature ne bouge pas. |
| `health` | Points de vie (10 par défaut). Un coup de joueur en retire 2,5 : la bête rougit, recule, puis s'enfuit ; à 0 elle bascule sur le flanc et reste à terre 10 minutes, le temps qu'on la dépèce. |
| `min_group`, `max_group` | Taille d'un groupe posé ensemble dans une zone (1 par défaut). |
| `temperament` | Réaction au joueur le plus proche, tant qu'il ne l'a pas frappée : `craintif` (fuit quand il approche à moins de 8 blocs, ou de 3 blocs s'il est accroupi avec Maj), `placide` (s'arrête et le regarde à moins de 6 blocs), `curieux` (le suit à moins de 16 blocs et s'arrête à 3 blocs de lui) ou `indifferent` (l'ignore, par défaut). |
| `attack` | Comportement agressif (voir plus bas). Sans ce champ, la bête est paisible : frappée, elle s'enfuit. |
| `guard` | Elle se garde : les coups qui lui arrivent de face, dans `arc` degrés (120 par défaut), perdent la part `block` de leurs dégâts (0,6 : 60 %), sauf quand elle est étourdie ou que Brise-Garde a brisé sa garde (garde, voyageur armé, chevalier de la mort). |
| `breeding` | Élevage : `food` (ce qu'elle mange, identifiants d'objets ou de blocs : tenu en main, il l'attire), `taming_chance` (chance qu'un repas l'apprivoise, 1/3 par défaut), `lays` (ponte d'une bête apprivoisée et adulte : `item`, l'objet pondu, et `minutes`, le temps entre deux pontes au moins et au plus) et `growth_minutes` (temps qu'il faut à un petit pour devenir adulte, 10 par défaut). Sans ce champ, elle ne s'élève pas. Exemple : `"breeding": {"food": ["grainesBle"], "growth_minutes": 6, "lays": {"item": "oeufFaisan", "minutes": [5, 10]}}`. |
| `spawn` | Les biomes où l'espèce apparaît d'elle-même (voir ci-dessous). Sans ce champ, elle n'apparaît jamais seule : c'est le cas des créatures des profondeurs, des mondes démoniaques et des bois féeriques, qui attendent leur monde. |
| `activity` | Le rythme de vie de l'espèce (voir ci-dessous, `diurnal` par défaut). |
| `loot` | Ce que donne la bête dépecée, au format du champ `loot` des blocs (`assets/blocks/README.md`) : `[{"item": "viandeVache", "count": [1, 3]}, {"item": "bone", "chance": 0.5}]`. Une bête tuée reste couchée à terre 10 minutes ; un clic droit sur elle, couteau ou dague en main, la dépèce : elle disparaît et son butin tombe à côté. En ligne, le serveur tire le butin et l'envoie à celui qui l'a dépecée. |
| `model` | Le modèle 3D (voir ci-dessous). |
| `magnet` | Aimant (Piège Magnétique) : `radius` (blocs) et `speed` (blocs par seconde au bord du rayon) ; les ennemis à portée sont tirés vers lui. |
| `collision` | Corps solide (voir plus bas). Sans ce champ : un disque de la demi-largeur du corps, de résistance 0,5. |

Chaque entrée de `spawn` a trois champs :
- `biome` : l'un de `Plains`, `Forest`, `Desert`, `Mountains`, `SnowPeaks`, `Tundra`, `Taiga`, `Savanna`, `Jungle`, `Beach`, `Sea`, `Ocean` ou `DeepOcean` ;
- `weight` : le poids de l'espèce face aux autres espèces du même biome ;
- `chance` (1 par défaut) : la probabilité que le groupe tiré apparaisse vraiment, pour une espèce rare.

Les créatures ne sont posées que sur l'herbe, la terre, le sable ou la neige. La population est tirée une seule fois par zone de 32 × 32 blocs, d'après la graine du monde, la première fois qu'un joueur s'en approche (une chance sur quatre d'y poser un groupe) ; ensuite rien ne réapparaît. Changer ces fichiers ne change que les zones pas encore visitées.

## Rythme jour/nuit

L'heure dépend de l'endroit : la planète est ronde, il fait jour d'un côté et nuit de l'autre. Pour chaque bête, c'est la hauteur du soleil au-dessus de son horizon qui compte. `activity` dit quand l'espèce est éveillée :

| Valeur | Éveillée | Exemples |
|---|---|---|
| `diurnal` (par défaut) | le jour, l'aube et le crépuscule | vache, mouflon, faisan, ours |
| `nocturnal` | la nuit, l'aube et le crépuscule | loup, sanglier, lézard, mille-pattes |
| `crepuscular` | autour du lever et du coucher du soleil (soleil à moins de 30° de l'horizon) | lapin, cerf |
| `always` | tout le temps | tréant, rampant, champion |

La population étant fixe, toutes les espèces sont là à toute heure. Hors de ses heures, une bête déjà là s'endort sur place à la fin de ce qu'elle faisait : un troupeau s'endort et se réveille peu à peu, pas d'un bloc. Un coup la réveille (et la fait fuir) ; elle se rendort plus tard si ce n'est toujours pas son heure.

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

| `shot` | Tir à distance : `range` (portée), `damage`, `speed` (blocs par seconde, 0 : tout de suite), `radius` (explosion), `height` (d'où part le tir) et les effets `muzzle`, `projectile`, `impact`. `pierce` (blocs) : le tir traverse et touche chaque ennemi près de sa trajectoire (Tourelle d'Exécution). Trop loin pour frapper, la bête s'arrête et tire. |
| `slow` | Son coup ralentit le joueur touché : `[part de la vitesse perdue, secondes]` (Gorgone florale, Mille-pattes géant). |
| `stun` | Son coup étourdit le joueur touché tant de secondes (Brute cornue, Sanglier). |
| `knockback` | Son coup repousse le joueur touché, à tant de blocs par seconde (Brute cornue, Sanglier, Ours). Une garde qui pare le coup, ou une immunité au recul, l'en protège. |
| `kind` | Type de ses coups : `physical` (par défaut) ou `magical` (souffle du chien de l'enfer, regard de la gorgone). L'armure portée par le joueur ne réduit que les coups physiques. |
| `burst` | Piège : la première cible à portée de `reach` le déclenche ; il retire `damage` à chaque ennemi dans `radius` blocs, l'immobilise `stun` secondes, le fait saigner (`bleed` points par seconde pendant `bleed_seconds` secondes, sans recul) et joue l'effet `effect`, puis disparaît. `spawn` (`species`, `count`, `seconds`) : ce qu'il fait apparaître autour de lui en se déclenchant, pour le même invocateur (les trois mines du Piège d'Éradication), qui disparaît au bout de `seconds` secondes si rien ne le déclenche. |

Le bestiaire ne décrit pas d'attaque : `tools/importer_bestiaire.py` garde celle de la fiche existante.

## Collisions

Les entités ne se traversent pas : chacune sort de ce qu'elle enfonce (une autre bête, un mur, un joueur), et le joueur sort des entités qu'il enfonce.

```json
"collision": {"shape": "box", "size": [2.62, 0.62], "height": 3.16, "resistance": 1}
```

| Champ | Rôle |
|---|---|
| `shape` | `circle` (disque, par défaut) ou `box` (rectangle tourné avec la bête). |
| `size` | Demi-largeur et demi-longueur, en blocs (le disque a pour rayon la première). Sans : celles de `body`. |
| `height` | Hauteur, en blocs (sans : celle de `body`). Ce qui passe au-dessus ne la touche pas. |
| `resistance` | De 0 à 1 : la part de l'enfoncement qu'elle repousse à chaque instant, entière si elle ne bouge pas (`walk_speed` 0), moitié sinon. 0 : on la traverse (pièges) ; 1 : infranchissable (murs). Un joueur a 0,5. |

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
