#!/usr/bin/env python3
"""Ecrit AtlasDonnees.java : les donnees declaratives de l'atlas des heros.

La maquette `maquette/hero-atlas.js` est la reference de l'atlas : patron
64x64, coiffures, yeux, bouches, marques, barbes, tenues, palettes et especes
avec leurs variantes. Le peintre en a ete porte en Java a la main, mais pas ses
donnees -- 75 Ko de tables recopiees a la main divergeraient a la premiere
retouche. Ce script les traduit, a partir de `maquette/atlas.json` que
`maquette/references.mjs` exporte tel quel :

    modules/HeroClasses/src/main/java/org/terasology/heroclasses/legende/AtlasDonnees.java

Le bac a sable des modules interdit de lire un fichier ou de charger Gson : les
donnees entrent donc dans le jar comme du code, une methode par table et une
par espece, pour rester loin de la borne de 64 Ko de bytecode par methode.

Une cle inconnue dans les traits ou les mods fait echouer le script : elle
n'aurait pas de champ dans `Traits` ou `Mods`, et se perdrait en silence.

Usage :
    node tools/heros/maquette/references.mjs <dossier>   # puis copier atlas.json
    python tools/heros/atlas.py
"""

import json
import pathlib
import sys

ICI = pathlib.Path(__file__).resolve().parent
RACINE = ICI.parents[1]
SOURCE = ICI / "maquette/atlas.json"
CIBLE = (RACINE / "modules/HeroClasses/src/main/java/org/terasology/heroclasses"
         "/legende/AtlasDonnees.java")

FACES = ["dessus", "dessous", "droite", "devant", "gauche", "derriere"]

# Type Java de chaque cle de trait et de mod : d (double), i (int),
# s (texte), b (booleen). C'est la liste des champs de Traits et de Mods.
TRAITS = {
    "relief": "d", "alpha": "d", "pelage": "d",
    "oreilles": "i", "canines": "i", "veines": "i", "barbeMin": "i",
    "motif": "s", "accent": "s", "reflet": "s", "lueur": "s", "regard": "s",
    "appendice": "s", "queue": "s", "oreillesAnimales": "s",
    "coiffureForcee": "s", "barbeForcee": "s", "yeuxForces": "s",
    "formeTeteForcee": "s", "boucheForcee": "s",
    "mainsMassives": "b", "machoire": "b", "machoireLourde": "b",
    "craquelee": "b", "chevelureEtheree": "b", "halo": "b", "griffes": "b",
}
MODS = {
    "torse": "d", "jambes": "d", "bras": "d", "tete": "d", "voute": "i",
    "flotte": "b", "ailes": "b", "pointes": "b",
}


# ------------------------------------------------------------------ litteraux
#
# json.dumps en ASCII donne des echappements \uXXXX que javac lit tels quels :
# le fichier genere reste en ASCII, quel que soit l'encodage du build.

def chaine(s):
    if s is None:
        return "null"
    return json.dumps(s, ensure_ascii=True)


def reel(v):
    return repr(float(v))


def entier(v):
    if isinstance(v, bool) or int(v) != v:
        raise ValueError("entier attendu : %r" % (v,))
    return str(int(v))


def valeur(v, genre):
    if genre == "d":
        return reel(v)
    if genre == "i":
        return entier(v)
    if genre == "s":
        return chaine(v)
    if genre == "b":
        if not isinstance(v, bool):
            raise ValueError("booleen attendu : %r" % (v,))
        return "true" if v else "false"
    raise ValueError(genre)


def ints(liste):
    return "new int[] {" + ", ".join(entier(v) for v in liste) + "}"


def textes(liste):
    return "List.of(" + ", ".join(chaine(s) for s in liste) + ")"


def fluide(classe, table, genres, ou):
    """`new Traits().relief(0.7).motif("taches")` : un appel par cle presente."""
    morceaux = ["new " + classe + "()"]
    for cle, v in table.items():
        if cle not in genres:
            sys.exit("cle inconnue %r dans %s (%s) : ajouter le champ a %s et a "
                     "atlas.py" % (cle, classe, ou, classe))
        morceaux.append(".%s(%s)" % (cle, valeur(v, genres[cle])))
    return "".join(morceaux)


def liste(elements, retrait):
    if not elements:
        return "List.of()"
    sep = ",\n" + " " * retrait
    return "List.of(\n" + " " * retrait + sep.join(elements) + ")"


# --------------------------------------------------------------------- tables

def rect(r):
    return "new Rect(%s, %s, %s, %s)" % (entier(r["x"]), entier(r["y"]),
                                         entier(r["largeur"]), entier(r["hauteur"]))


def methode_patron(patron):
    corps = []
    for nom, p in patron["parties"].items():
        if list(p["faces"]) != FACES:
            sys.exit("faces de %s hors de l'ordre canonique" % nom)
        b = p["boite"]
        corps.append(
            "        parties.put(%s, new Partie(%s, %s, %s, %s, %s, %s, %s,\n"
            "                faces(%s)));" % (
                chaine(nom), chaine(nom), chaine(p["os"]), entier(p["couche"]),
                entier(b["largeur"]), entier(b["hauteur"]), entier(b["profondeur"]),
                reel(p["gonflement"]),
                ",\n                        ".join(rect(p["faces"][f]) for f in FACES)))
    return ("    static Map<String, Partie> patron() {\n"
            "        Map<String, Partie> parties = new LinkedHashMap<>();\n"
            + "\n".join(corps) + "\n"
            "        return Collections.unmodifiableMap(parties);\n"
            "    }\n")


def touche(op):
    x, y, l, h, t = op
    return "new Touche(%s, %s, %s, %s, %s)" % (entier(x), entier(y), entier(l), entier(h), chaine(t))


def pose(op):
    p, x, y, l, h = op
    return "new Pose(%s, %s, %s, %s, %s)" % (chaine(p), entier(x), entier(y), entier(l), entier(h))


def coiffure(c):
    def profil(cle):
        return ints(c[cle]) if cle in c else "null"
    extras = liste([pose(e) for e in c.get("extras", [])], 20)
    return "new Coiffure(%s, %s, %s, %s, %s, %s,\n                %s)" % (
        chaine(c["id"]), chaine(c["nom"]), profil("dev"), profil("cot"), profil("der"),
        chaine(c.get("chute")), extras)


def expression(e):
    return "new Expression(%s, %s, %s)" % (
        chaine(e["id"]), chaine(e["nom"]), liste([touche(o) for o in e["ops"]], 20))


def marque(m):
    return "new Marque(%s, %s, %s, %s)" % (
        chaine(m["id"]), chaine(m["nom"]), chaine(m.get("teinte")),
        liste([pose(o) for o in m.get("ops", [])], 20))


def barbe(b):
    if "rangs" not in b:
        rangs = "null"
    else:
        rangs = "new int[][][] {" + ", ".join(
            "{" + ", ".join("{%s, %s}" % (entier(x), entier(w)) for x, w in bandes) + "}"
            for bandes in b["rangs"]) + "}"
    return "new Barbe(%s, %s,\n                %s)" % (chaine(b["id"]), chaine(b["nom"]), rangs)


def tenue(t):
    connus = {"id", "nom", "ton", "manche", "jambes", "col", "ceinture", "epaulettes",
              "torseNu", "extras"}
    inconnus = set(t) - connus
    if inconnus:
        sys.exit("cles de tenue inconnues : %s" % sorted(inconnus))

    def drapeau(cle):
        return "true" if t.get(cle) else "false"
    return "new Tenue(%s, %s, %s, %s, %s, %s, %s, %s, %s,\n                %s)" % (
        chaine(t["id"]), chaine(t["nom"]), chaine(t["ton"]), entier(t["manche"]),
        chaine(t["jambes"]), drapeau("col"), drapeau("ceinture"), drapeau("epaulettes"),
        drapeau("torseNu"), liste([touche(o) for o in t.get("extras", [])], 20))


def methode_liste(nom, type_, elements):
    return ("    static List<%s> %s() {\n"
            "        return List.of(\n"
            "                %s);\n"
            "    }\n" % (type_, nom, ",\n                ".join(elements)))


def methode_espece(e):
    variantes = []
    for i, v in enumerate(e["variantes"]):
        ou = "%s.variantes[%d]" % (e["id"], i)
        variantes.append(
            "new Variante(%s,\n"
            "                                %s,\n"
            "                                %s,\n"
            "                                %s,\n"
            "                                %s)" % (
                chaine(v["nom"]), chaine(v.get("apparence")),
                textes(v["peaux"]) if "peaux" in v else "null",
                fluide("Mods", v.get("mods", {}), MODS, ou),
                fluide("Traits", v.get("traits", {}), TRAITS, ou)))
    return (
        "    private static Espece %s() {\n"
        "        return new Espece(%s, %s, %s, %s, %s,\n"
        "                %s,\n"
        "                %s,\n"
        "                %s,\n"
        "                %s,\n"
        "                %s,\n"
        "                %s);\n"
        "    }\n" % (
            "espece" + e["id"][0].upper() + e["id"][1:],
            chaine(e["id"]), chaine(e["nom"]), reel(e["taille"]), reel(e["min"]), reel(e["max"]),
            chaine(e.get("apparence")), chaine(e.get("attribut")), textes(e["peaux"]),
            fluide("Mods", e.get("mods", {}), MODS, e["id"]),
            fluide("Traits", e.get("traits", {}), TRAITS, e["id"]),
            liste(variantes, 24)))


# ------------------------------------------------------------------- fichier

ENTETE = """\
// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
// GENERATED by tools/heros/atlas.py from tools/heros/maquette/atlas.json -- do not edit by hand.
package org.terasology.heroclasses.legende;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * The atlas's declarative data, translated from the reference mock-up {@code hero-atlas.js}.
 * <p>
 * Generated: modules cannot read files nor parse JSON, so the data ships as code, one method per table and per
 * species to stay far below the 64 KB bytecode limit of a method. {@link Atlas} is the public face.
 */
final class AtlasDonnees {

    static final String IMAGE = %s;
    static final double UNITE_EN_BLOCS = %s;

    private AtlasDonnees() {
    }

    private static Map<String, Rect> faces(Rect dessus, Rect dessous, Rect droite, Rect devant, Rect gauche,
                                           Rect derriere) {
        Map<String, Rect> faces = new LinkedHashMap<>();
        faces.put("dessus", dessus);
        faces.put("dessous", dessous);
        faces.put("droite", droite);
        faces.put("devant", devant);
        faces.put("gauche", gauche);
        faces.put("derriere", derriere);
        return Collections.unmodifiableMap(faces);
    }

"""


def generer(atlas):
    if atlas["FACES"] != FACES:
        sys.exit("FACES a change : %r" % (atlas["FACES"],))
    morceaux = [ENTETE % (chaine(atlas["PATRON"]["image"]), reel(atlas["PATRON"]["unite_en_blocs"]))]
    morceaux.append(methode_patron(atlas["PATRON"]))
    morceaux.append(methode_liste("faces", "String", [chaine(f) for f in atlas["FACES"]]))
    morceaux.append(methode_liste("coiffures", "Coiffure", [coiffure(c) for c in atlas["COIFFURES"]]))
    morceaux.append(methode_liste("yeux", "Expression", [expression(e) for e in atlas["YEUX"]]))
    morceaux.append(methode_liste("bouches", "Expression", [expression(e) for e in atlas["BOUCHES"]]))
    morceaux.append(methode_liste("marques", "Marque", [marque(m) for m in atlas["MARQUES"]]))
    morceaux.append(methode_liste("barbes", "Barbe", [barbe(b) for b in atlas["BARBES"]]))
    morceaux.append(methode_liste("tenues", "Tenue", [tenue(t) for t in atlas["TENUES"]]))
    morceaux.append(methode_liste("formesTete", "FormeTete",
                                  ["new FormeTete(%s, %s)" % (chaine(f["id"]), chaine(f["nom"]))
                                   for f in atlas["FORMES_TETE"]]))
    for nom, cle in (("palCheveux", "PAL_CHEVEUX"), ("palYeux", "PAL_YEUX"), ("palTenue", "PAL_TENUE")):
        morceaux.append(methode_liste(nom, "String", [chaine(c) for c in atlas[cle]]))
    morceaux.append(methode_liste("ordreEspeces", "String", [chaine(s) for s in atlas["ORDRE_ESPECES"]]))
    especes = list(atlas["ESPECES"].values())
    morceaux.append(methode_liste("especes", "Espece",
                                  ["espece" + e["id"][0].upper() + e["id"][1:] + "()" for e in especes]))
    for e in especes:
        morceaux.append(methode_espece(e))
    return "\n".join(morceaux).rstrip("\n") + "\n}\n"


def main():
    atlas = json.loads(SOURCE.read_text(encoding="utf-8"))
    texte = generer(atlas)
    CIBLE.parent.mkdir(parents=True, exist_ok=True)
    CIBLE.write_text(texte, encoding="ascii", newline="\n")
    print("%s : %d octets" % (CIBLE.relative_to(RACINE), len(texte)))


if __name__ == "__main__":
    main()
