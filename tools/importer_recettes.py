#!/usr/bin/env python3
"""Reprend les recettes du module `HeroCrafting` (branche `develop`) dans les fiches de ce qu'elles
fabriquent : le champ `recipes` des fiches de `assets/items/` et `assets/blocks/`.

    python3 tools/importer_recettes.py CHEMIN/modules

Pour partir de la branche `develop` sans la sortir :

    git worktree add /tmp/develop origin/develop
    python3 tools/importer_recettes.py /tmp/develop/modules

Chaque prefab `prefabs/recipes/Recipe*.prefab` devient une recette : ses ingrédients (`3*Module:Nom`,
plusieurs noms séparés par `|` au choix), le nombre fabriqué, l'établi (`station`) et ce qu'il faut
pour réparer. Les établis de `develop` deviennent ceux de `assets/stations/` : la forge astrale
ultime y est le niveau 2 de la forge astrale.

Les recettes déjà présentes dans une fiche sont remplacées par celles de `develop` ; une fiche que
`develop` ne fabrique pas garde les siennes (recettes écrites à la main).
"""
import json
import pathlib
import sys

from importer_blocs_objets import BLOCS, OBJETS, ecrire, ident, lire_json

# Établi de develop -> (établi de assets/stations/, niveau)
ETABLIS = {
    "workbench": ("workbench", 1),
    "furnace": ("furnace", 1),
    "lapidary": ("lapidary", 1),
    "astralforge": ("astralForge", 1),
    "ultimateforge": ("astralForge", 2),
}


def ingredients(liste):
    """`["3*CoreAssets:Plank", "1*A:X|A:Y"]` -> `{"plank": 3, "x|y": 1}`"""
    sortie = {}
    for texte in liste:
        nombre, _, uris = texte.partition("*")
        cle = "|".join(ident(u.strip().split(":")[1]) for u in uris.split("|"))
        sortie[cle] = sortie.get(cle, 0) + int(nombre)
    return sortie


def main(racine):
    racine = pathlib.Path(racine)
    recettes = {}
    for chemin in sorted(racine.glob("*/assets/prefabs/recipes/*.prefab")):
        r = lire_json(chemin).get("Recipe")
        if not r:
            continue
        resultat = ident(r["result"].split(":")[1])
        recette = {}
        if r.get("station"):
            etabli, niveau = ETABLIS[r["station"]]
            recette["station"] = etabli
            if niveau > 1:
                recette["level"] = niveau
        recette["ingredients"] = ingredients(r["ingredients"])
        if r.get("count", 1) != 1:
            recette["count"] = r["count"]
        if r.get("repair"):
            recette["repair"] = ingredients(r["repair"])
        recettes.setdefault(resultat, []).append(recette)

    ecrites = 0
    for resultat, liste in sorted(recettes.items()):
        chemin = next((d / f"{resultat}.json" for d in (OBJETS, BLOCS) if (d / f"{resultat}.json").exists()), None)
        if chemin is None:
            print(f"Recette de {resultat} : aucune fiche, ignorée")
            continue
        fiche = json.loads(chemin.read_text(encoding="utf-8"))
        fiche["recipes"] = liste
        ecrire(chemin, fiche)
        ecrites += len(liste)
    print(f"{ecrites} recettes écrites dans {len(recettes)} fiches")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    sys.path.insert(0, str(pathlib.Path(__file__).parent))
    main(sys.argv[1])
