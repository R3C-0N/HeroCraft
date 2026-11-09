#!/usr/bin/env python3
"""Vérifie les effets visuels (assets/effects/) et les gestes du héros (assets/hero/animations/).

En attendant le moteur qui les jouera, ce script relit chaque fichier et signale ce qui ne
suit pas les README des deux dossiers : champ inconnu, valeur hors des listes permises, paire
mal formée, texture absente, os inconnu, clés dans le désordre. Il sort en erreur s'il trouve
quoi que ce soit.

    python3 tools/verifier_effets_gestes.py
"""

import json
import re
import sys
from pathlib import Path

RACINE = Path(__file__).resolve().parent.parent
EFFETS = RACINE / "assets" / "effects"
GESTES = RACINE / "assets" / "hero" / "animations"

COULEUR = re.compile(r"^#[0-9A-Fa-f]{6}([0-9A-Fa-f]{2})?$")

TEMPS_COUCHE = {"delay", "duration", "loop", "fade_in", "fade_out"}
FORMES = {
    "ring": {"radius", "thickness", "on_ground", "upright"},
    "disc": {"radius", "spin", "texture", "on_ground", "upright"},
    "arc": {"radius", "angle", "thickness", "sweep"},
    "streak": {"length", "width"},
    "dome": {"radius", "pulse"},
    "column": {"radius", "height"},
    "sprite": {"size", "texture", "spin"},
    "trail": {"width", "life"},
}
COMMUN_FORME = {"color", "glow", "pulse", "offset"}
PARTICULES = {
    "count", "rate", "origin", "origin_radius", "origin_height", "origin_length", "upright",
    "direction", "spread", "speed", "life", "gravity", "drag", "size", "color", "glow",
    "texture", "spin", "orbit", "offset", "block_color",
}
ORIGINES = {"point", "sphere", "ring", "disc", "cylinder", "line"}
DIRECTIONS = {"up", "outward", "inward", "forward", "random", "none"}

OS = {
    "body", "torso", "head", "rightArm", "leftArm", "rightForearm", "leftForearm",
    "rightHand", "leftHand", "rightLeg", "leftLeg", "rightShin", "leftShin",
}
EASES = {"inOut", "in", "out", "linear"}


def nombre(v):
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def valeur(v):
    """Un nombre, ou une paire de nombres."""
    return nombre(v) or (isinstance(v, list) and len(v) == 2 and all(nombre(x) for x in v))


def couleur(v):
    return isinstance(v, list) and len(v) == 2 and all(isinstance(c, str) and COULEUR.match(c) for c in v)


def verifier_valeurs(lieu, champs, erreurs):
    for cle, v in champs.items():
        if cle == "color":
            if not couleur(v):
                erreurs.append(f"{lieu} : color doit être une paire de couleurs #RRGGBB[AA]")
        elif cle == "texture":
            if not (EFFETS / "textures" / str(v)).is_file():
                erreurs.append(f"{lieu} : texture introuvable {v}")
        elif cle in ("on_ground", "upright", "sweep", "loop", "block_color"):
            if not isinstance(v, bool):
                erreurs.append(f"{lieu} : {cle} doit valoir true ou false")
        elif cle == "offset":
            if not (isinstance(v, list) and len(v) == 3 and all(nombre(x) for x in v)):
                erreurs.append(f"{lieu} : offset doit valoir [gauche, haut, avant]")
        elif cle == "origin":
            if v not in ORIGINES:
                erreurs.append(f"{lieu} : origin inconnue {v}")
        elif cle == "direction":
            if v not in DIRECTIONS:
                erreurs.append(f"{lieu} : direction inconnue {v}")
        elif not valeur(v):
            erreurs.append(f"{lieu} : {cle} doit être un nombre ou une paire de nombres")


def verifier_effet(chemin, effet, erreurs):
    nom = chemin.name
    for cle in effet:
        if cle not in ("description", "unit", "layers"):
            erreurs.append(f"{nom} : champ inconnu {cle}")
    if effet.get("unit", "blocks") not in ("blocks", "area"):
        erreurs.append(f"{nom} : unit vaut blocks ou area")
    couches = effet.get("layers")
    if not isinstance(couches, list) or not couches:
        erreurs.append(f"{nom} : layers doit être une liste non vide")
        return
    for i, couche in enumerate(couches):
        lieu = f"{nom}, couche {i + 1}"
        if ("shape" in couche) == ("particles" in couche):
            erreurs.append(f"{lieu} : une couche a soit shape, soit particles")
            continue
        temps = {k: v for k, v in couche.items() if k in TEMPS_COUCHE}
        verifier_valeurs(lieu, temps, erreurs)
        if "shape" in couche:
            forme = couche["shape"]
            if forme not in FORMES:
                erreurs.append(f"{lieu} : forme inconnue {forme}")
                continue
            champs = {k: v for k, v in couche.items() if k not in TEMPS_COUCHE and k != "shape"}
            for cle in champs:
                if cle not in FORMES[forme] | COMMUN_FORME:
                    erreurs.append(f"{lieu} : champ {cle} inconnu pour la forme {forme}")
            verifier_valeurs(lieu, champs, erreurs)
            if "color" not in champs:
                erreurs.append(f"{lieu} : color manquante")
        else:
            autres = set(couche) - TEMPS_COUCHE - {"particles"}
            for cle in sorted(autres):
                erreurs.append(f"{lieu} : champ {cle} inconnu à côté de particles")
            p = couche["particles"]
            for cle in p:
                if cle not in PARTICULES:
                    erreurs.append(f"{lieu} : champ de particules inconnu {cle}")
            if ("count" in p) == ("rate" in p):
                erreurs.append(f"{lieu} : des particules ont soit count, soit rate")
            if "rate" in p and not (couche.get("loop") or "duration" in couche):
                erreurs.append(f"{lieu} : un rate demande loop ou duration")
            for requis in ("origin", "life", "size", "color"):
                if requis not in p:
                    erreurs.append(f"{lieu} : {requis} manquant")
            verifier_valeurs(lieu, p, erreurs)


def verifier_geste(chemin, geste, erreurs):
    nom = chemin.name
    for cle in geste:
        if cle not in ("description", "duration", "legs", "impact", "loop", "keys"):
            erreurs.append(f"{nom} : champ inconnu {cle}")
    if not (nombre(geste.get("duration")) and geste["duration"] > 0):
        erreurs.append(f"{nom} : duration doit être un nombre positif")
    if geste.get("legs") not in ("free", "locked"):
        erreurs.append(f"{nom} : legs vaut free ou locked")
    if "impact" in geste and not (nombre(geste["impact"]) and 0 <= geste["impact"] <= 1):
        erreurs.append(f"{nom} : impact va de 0 à 1")
    boucle = geste.get("loop")
    if boucle is not None and not (isinstance(boucle, list) and len(boucle) == 2
                                   and all(nombre(x) for x in boucle) and 0 <= boucle[0] < boucle[1] <= 1):
        erreurs.append(f"{nom} : loop vaut [début, fin] avec 0 ≤ début < fin ≤ 1")
    cles = geste.get("keys")
    if not isinstance(cles, list) or len(cles) < 2:
        erreurs.append(f"{nom} : keys demande au moins deux clés")
        return
    precedent = -1.0
    for i, cle in enumerate(cles):
        lieu = f"{nom}, clé {i + 1}"
        t = cle.get("t")
        if not (nombre(t) and 0 <= t <= 1 and t > precedent):
            erreurs.append(f"{lieu} : t doit croître de 0 à 1")
        else:
            precedent = t
        for champ in cle:
            if champ not in ("t", "ease", "bones"):
                erreurs.append(f"{lieu} : champ inconnu {champ}")
        if cle.get("ease", "inOut") not in EASES:
            erreurs.append(f"{lieu} : ease inconnu {cle.get('ease')}")
        for os_, pose in cle.get("bones", {}).items():
            if os_ not in OS:
                erreurs.append(f"{lieu} : os inconnu {os_}")
            for champ, v in pose.items():
                if champ not in ("rotation", "translation"):
                    erreurs.append(f"{lieu}, {os_} : champ inconnu {champ}")
                elif champ == "translation" and os_ != "body":
                    erreurs.append(f"{lieu}, {os_} : seul body se déplace")
                elif not (isinstance(v, list) and len(v) == 3 and all(nombre(x) for x in v)):
                    erreurs.append(f"{lieu}, {os_} : {champ} vaut [x, y, z]")
    if cles[0].get("t") != 0 or cles[-1].get("t") != 1:
        erreurs.append(f"{nom} : la première clé est à t = 0 et la dernière à t = 1")


def main():
    erreurs = []
    compte = {}
    for dossier, verifier in ((EFFETS, verifier_effet), (GESTES, verifier_geste)):
        fichiers = sorted(dossier.glob("*.json"))
        compte[dossier.name] = len(fichiers)
        for chemin in fichiers:
            try:
                contenu = json.loads(chemin.read_text(encoding="utf-8"))
            except json.JSONDecodeError as e:
                erreurs.append(f"{chemin.name} : JSON invalide ({e})")
                continue
            verifier(chemin, contenu, erreurs)
    for erreur in erreurs:
        print(erreur)
    print(f"{compte['effects']} effet(s), {compte['animations']} geste(s), {len(erreurs)} erreur(s).")
    sys.exit(1 if erreurs else 0)


if __name__ == "__main__":
    main()
