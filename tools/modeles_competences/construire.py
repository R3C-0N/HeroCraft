#!/usr/bin/env python3
"""Construit les modèles 3D des compétences et leurs fiches :

- `assets/creatures/<id>.json` et `assets/creatures/modeles/<id>.gltf|png` : formes du Druide et
  créatures invoquées par le Nécromancien (sans `spawn` : elles n'apparaissent jamais seules) ;
- `assets/skills/models/<id>.gltf|png` : objets posés (tourelles, pièges, murs…), pièces greffées
  sur le héros et armes tenues ;
- `assets/skills/models/<id>.png` (planches 64 × 64) et `assets/skills/models/humanoids.json` :
  invocations humanoïdes sur le corps du héros, leurre et ombres teintés.

    python3 tools/modeles_competences/construire.py

Les modèles sont décrits en Python (pavés, os, couleurs, motifs) dans les fichiers voisins :
`druide.py`, `necromancien.py`, `ingenieur.py`, `greffes.py`, `humanoides.py`. Les relancer
régénère tout à l'identique (les textures sont tirées d'une graine fixe).
"""
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))

import druide  # noqa: E402
import especes  # noqa: E402
import greffes  # noqa: E402
import humanoides  # noqa: E402
import ingenieur  # noqa: E402
import necromancien  # noqa: E402
from importer_bestiaire import ecrire  # noqa: E402

ROOT = HERE.parents[1]
CREATURES = ROOT / "assets/creatures"
MODELS = ROOT / "assets/skills/models"

CREATURE_MODELS = druide.ALL + necromancien.ALL[:3]
OBJECT_MODELS = necromancien.ALL[3:] + ingenieur.ALL + greffes.ALL + humanoides.WEAPONS

# Les invocations humanoïdes : planche, espèce dont elles prennent les proportions, arme
HUMANOIDS = {
    "deathKnight": {"name": "Chevalier de la Mort", "sheet": "deathKnight.png", "species": "demiOrque", "weapon": "runeblade"},
    "lichLord": {"name": "Seigneur Liche", "sheet": "lichLord.png", "species": "revenant", "weapon": "lichStaff"},
}
# Copies teintées : la couleur multipliée à la planche du héros (ou du cadavre), comme `FigurePose::tint`
TINTS = {
    "shadowDecoy": {"name": "Leurre Obscur", "copies": "caster", "tint": [0.16, 0.13, 0.22]},
    "etheriumShade": {"name": "Ombre d'Étherium", "copies": "corpse", "tint": [0.45, 0.62, 1.25]},
}

# Fiches des créatures du Nécromancien (les formes du Druide sont dans `especes.py`)
NECRO_SPECIES = {
    "fleshGolem": {
        "name": "Golem de Chair", "description": "Une masse de chairs recousues qui avance en écrasant tout.",
        "body": {"half_width": 0.75, "half_length": 0.5, "height": 2.7}, "walk_speed": 1.2, "health": 80,
        "model": "biped"},
    "skeletonServant": {
        "name": "Serviteur Squelette", "description": "Des os rassemblés par la nécromancie, une épée rouillée à la main.",
        "body": {"half_width": 0.3, "half_length": 0.3, "height": 1.95}, "walk_speed": 1.8, "health": 20,
        "model": "biped"},
    "boneBehemoth": {
        "name": "Béhémoth d'Os", "description": "Un colosse d'os assemblés, une âme verte prise dans la cage.",
        "body": {"half_width": 0.8, "half_length": 2.3, "height": 3.3}, "walk_speed": 1.4, "health": 140,
        "model": "quadruped"},
}


def golem_animations():
    a = especes.biped(0.8, 18)
    for anim in a.values():
        os_ = anim["os"]
        if "chest" in os_:
            os_["torso"] = os_.pop("chest")
    a["attaque"] = {"cycle": 1.2, "os": {
        "armR": {"rotation": [-60, 0, 0], "rotation_fixe": [-60, 0, 0]}, "torso": {"rotation": [10, 0, 0]},
        "armL": {"rotation": [20, 0, 0], "phase": 0.5}}}
    return a


def main():
    MODELS.mkdir(parents=True, exist_ok=True)
    (CREATURES / "modeles").mkdir(parents=True, exist_ok=True)
    fiches = especes.species()
    fiches["fleshGolem"] = dict(NECRO_SPECIES["fleshGolem"], model=golem_animations())
    skeleton = especes.biped(0.8, 28)
    skeleton["attaque"] = {"cycle": 0.8, "os": {
        "armR": {"rotation": [-50, 0, 0], "rotation_fixe": [-30, 0, 0]}, "chest": {"rotation": [0, 15, 0]},
        "head": {"rotation": [6, 0, 0], "frequence": 2}}}
    skeleton["claque"] = {"cycle": 1.2, "pause": {"poids": 0.5, "duree": [1, 2]}, "os": {
        "head": {"rotation": [8, 0, 3], "frequence": 4}, "armL": {"rotation": [6, 0, 4], "frequence": 2}}}
    fiches["skeletonServant"] = dict(NECRO_SPECIES["skeletonServant"], model=skeleton)
    fiches["boneBehemoth"] = dict(NECRO_SPECIES["boneBehemoth"], model=especes.quadruped(1.1, 18, extra={"charge": {
        "cycle": 1.6, "pause": {"poids": 0.3, "duree": [1.5, 2.5]}, "os": {
            "head": {"rotation": [6, 0, 0], "rotation_fixe": [25, 0, 0], "frequence": 2},
            "root": {"rotation_fixe": [6, 0, 0]}, "legFR": {"rotation": [-20, 0, 0], "frequence": 2},
            "legFL": {"rotation": [-20, 0, 0], "frequence": 2, "phase": 0.5}}}}))
    for build in CREATURE_MODELS:
        m = build()
        m.export(CREATURES / "modeles" / f"{m.ident}.gltf", CREATURES / "modeles" / f"{m.ident}.png")
        fiche = dict(fiches[m.ident])
        animations = fiche.pop("model")
        fiche["model"] = {"mesh": f"modeles/{m.ident}.gltf", "texture": f"modeles/{m.ident}.png", "animations": animations}
        ecrire(CREATURES / f"{m.ident}.json", fiche)
        print("créature", m.ident, len(m.boxes), "pavés")
    for build in OBJECT_MODELS:
        m = build()
        m.export(MODELS / f"{m.ident}.gltf", MODELS / f"{m.ident}.png")
        print("modèle", m.ident, len(m.boxes), "pavés", "clips :", ", ".join(m.clips) or "aucun")
    for build in humanoides.SHEETS:
        s = build()
        s.save(MODELS / f"{s.ident}.png")
        print("planche", s.ident)
    (MODELS / "humanoids.json").write_text(json.dumps({"humanoids": HUMANOIDS, "tinted": TINTS}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
