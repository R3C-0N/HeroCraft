# Modèles des compétences

Les modèles 3D dont les compétences ont besoin, en plus de leurs effets visuels. Ils sont tous construits par `tools/modeles_competences/construire.py` (pavés, os, couleurs et motifs décrits en Python) : on retouche le script, puis on le relance.

```bash
python3 tools/modeles_competences/construire.py
```

## Créatures (`assets/creatures/`)

Les formes du Druide et les créatures invoquées par le Nécromancien sont des espèces comme les autres (fiche, glTF, texture, animations par os : voir `assets/creatures/README.md`). Elles n'ont pas de champ `spawn` : elles n'apparaissent jamais seules, seule une compétence les fait venir.

Une compétence les fait apparaître par l'effet `summon` de son bloc `cast` :

```json
{"summon": {"species": "skeletonServant", "count": [1, 2, 3], "at": "aimed", "duration": 45, "limit": [1, 2, 3]}}
```

| Champ | Rôle |
|---|---|
| `species` | L'espèce (`assets/creatures/<id>.json`). |
| `count` | Combien à chaque lancement (1 par défaut), posées en cercle. |
| `at` | `self` : aux pieds du lanceur ; `aimed` : au centre de la zone (le point visé). |
| `duration` | Secondes avant qu'elles disparaissent ; sans : jusqu'à leur destruction. |
| `limit` | Nombre au plus en même temps pour un lanceur : les plus anciennes disparaissent. |

Les entités invoquées sont des alliées du lanceur ; elles disparaissent quand il quitte la partie et ne sont jamais sauvegardées. Le Chevalier de la Mort et le Seigneur Liche sont des créatures humanoïdes (`humanoid: true`) du même identifiant que leur entrée de `humanoids.json` : le client les dessine avec le corps du héros, leur planche et leur arme greffée dans la main droite, et ils frappent les monstres qui approchent. Le comportement propre des autres entités (la tourelle qui tire, le piège qui claque, le mur qui bloque) n'est pas encore écrit : elles apparaissent, encaissent les coups et disparaissent.

| Identifiant | Compétence | Os |
|---|---|---|
| `forestWolf` | Loup des Bois | `root`, `head`, `tail`, `legFR`, `legFL`, `legBR`, `legBL` |
| `runicBear` | Ours Runique | idem |
| `ancientStag` | Grand Cerf Ancien | idem |
| `peregrineFalcon` | Faucon Pèlerin | `root`, `head`, `tail`, `wingR`, `wingL`, `legR`, `legL` |
| `owlbear` | Ours-Hibou | comme le loup |
| `wendigo` | Wendigo | `root`, `chest`, `head`, `armR`, `armL`, `legR`, `legL` |
| `sylvanGriffin` | Griffon Sylvestre | comme le loup, plus `wingR`, `wingL` |
| `voidChimera` | Avatar du Néant | idem |
| `skeletonServant` | Serviteur Squelette (os seuls, épée dans la main droite) | `root`, `chest`, `head`, `armR`, `armL`, `legR`, `legL` |
| `fleshGolem` | Golem de Chair | `root`, `torso`, `head`, `armR`, `armL`, `legR`, `legL` |
| `boneBehemoth` | Béhémoth d'Os | comme le loup |

## Objets posés (`*.gltf` de ce dossier)

Tourelles, pièges, mines et murs sont eux aussi des entités : chacun a sa fiche d'espèce dans `assets/creatures/<identifiant>.json` (immobile, `walk_speed` 0, toujours éveillé), qui pointe vers le glTF de ce dossier (`../skills/models/<identifiant>.gltf`). Le repos est la pose finie du modèle (mur dressé, barricade dépliée).

Même format que les créatures (glTF à squelette, données en base64, en blocs, y vers le haut, l'avant vers +z, posés sur le sol), avec leurs animations dans le glTF :

| Identifiant | Compétence | Animations (os) |
|---|---|---|
| `ballisticTurret` | Tourelle Balistique | `fire` (`head` pivote pour viser) |
| `flameTurret` | Tourelle Incendiaire | `fire` (`head`) |
| `jawTrap` | Piège à Mâchoire | `snap` (`jawR`, `jawL`) |
| `magneticTrap` | Piège Magnétique | `spin` (`ring`) |
| `shrapnelMine` | Mine à Éclats | `blink` (`light`) |
| `portableBarricade` | Barricade Portative | `unfold` (`panelR`, `panelL`) |
| `etheriumBastion` | Bastion d'Étherium | `rise`, `fire` (`cannonNE`, `cannonNW`, `cannonSE`, `cannonSW`) |
| `stoneWall` | Rempart Tellurique ; Mur de Magma teinté de rouge | `rise` |
| `boneWall` | Mur d'Ossements | `rise` |

Les murs de lumière (Chapelain, Champion, Paladin) n'ont pas de modèle : ce sont des effets, avec un collisionneur.

## Pièces greffées sur le héros

Les transformations partielles du Druide et les armes des invocations humanoïdes. Leurs os portent les noms des os du héros (`Bone::name` dans `crates/client/src/hero_model.rs`) et elles sont modelées sur le héros humain au repos, en blocs pour un héros de 1,8 bloc (1 texel du patron = 0,05625 bloc). Un sommet de l'os `B` se pose comme la figure : `figure.bone_frames(pose)[B].apply(position / 0.05625)` (voir `push_graft` dans `crates/client/src/skill_models.rs`).

| Identifiant | Compétence | Os |
|---|---|---|
| `wolfPaw` | Griffes de la Meute (les deux bras) | bras, avant-bras et mains |
| `bearHide` | Échine du Plantigrade | `torso`, `rightArm`, `leftArm` |
| `elkLegs` | Jambes de l'Élan | `rightLeg`, `leftLeg`, `rightShin`, `leftShin` |
| `galeFeathers` | Plumes de Bourrasque | bras, avant-bras et mains |
| `runeblade`, `lichStaff` | armes des invocations humanoïdes | `rightHand` |

## Invocations humanoïdes (`humanoids.json`)

Sur le corps du héros (13 os, gestes des compétences) : une planche 64 × 64 au patron du héros, l'espèce dont elles prennent les proportions et l'arme greffée sur la main droite.

| Identifiant | Planche | Espèce | Arme |
|---|---|---|---|
| `deathKnight` | `deathKnight.png` | `demiOrque` | `runeblade` |
| `lichLord` | `lichLord.png` | `revenant` | `lichStaff` |

Les copies teintées n'ont pas de modèle à elles : le Leurre Obscur (`shadowDecoy`) reprend la figure du lanceur, les Ombres d'Étherium (`etheriumShade`) celle du cadavre, avec la couleur `tint` multipliée à la planche (`FigurePose::tint`). L'Abomination du Berserker est le héros agrandi.

## Aperçus

```bash
# Héros : invocations armées, copies teintées, greffes
APERCU=heros.png cargo test -p herocraft_client apercu_competences_heros -- --ignored
# Créatures, avec leurs animations
APERCU=formes.png APERCU_ESPECES=forestWolf,runicBear APERCU_CASE=260 cargo test -p herocraft_client apercu_animations -- --ignored
# Tous les glTF, hors jeu
python3 tools/modeles_competences/apercu.py planche.png assets/skills/models/*.gltf
```
