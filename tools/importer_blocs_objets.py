#!/usr/bin/env python3
"""Reprend les blocs et les objets des modules Terasology (branche `develop`) dans `assets/blocks/`
et `assets/items/` : une fiche JSON par bloc ou par objet, au format de HeroCraft, plus les textures
des blocs et les icônes des objets.

    python3 tools/importer_blocs_objets.py CHEMIN/modules

Pour partir de la branche `develop` sans la sortir :

    git worktree add /tmp/develop origin/develop
    python3 tools/importer_blocs_objets.py /tmp/develop/modules

Blocs : les fichiers `.block` (sauf les modèles `templates/`), avec leurs modèles `basedOn` résolus.
Sont repris le nom, la catégorie, la dureté, la lumière, la forme (cube ou plante), le passage
(liquide, traversable), la transparence, le son des pas et les textures ; la couleur des faces est
la moyenne de leur texture. Les 14 blocs que le générateur de terrain pose gardent leur numéro et
leur couleur actuels.

Objets : les prefabs `engine:iconItem`, avec nom, description, taille de pile, icône (découpée dans
son atlas au besoin) et caractéristiques (dégâts, durabilité, protection, satiété).

Le numéro d'une fiche déjà présente est conservé : relancer le script après une retouche des modules
remet les fiches à jour sans changer les numéros enregistrés dans les mondes et les inventaires.
"""
import json
import pathlib
import re
import shutil
import sys

from PIL import Image

RACINE = pathlib.Path(__file__).resolve().parents[1]
BLOCS = RACINE / "assets/blocks"
OBJETS = RACINE / "assets/items"
# Premier numéro d'objet : les blocs sont en dessous (`herocraft_common::FIRST_OBJECT_ID`)
PREMIER_OBJET = 0x4000

# Blocs du générateur de terrain (`BlockId::STONE`…) : numéro et couleurs (dessus, côtés) actuels
# Formes des fichiers .shape de develop -> formes en pavés de assets/blocks/shapes/
FORMES = {
    "engine:verticalOffsetPlane": "panneau",
    "CoreAssets:cactus": "colonne",
}

BLOCS_DU_TERRAIN = {
    "stone": (1, "#85807a", "#85807a"),
    "dirt": (2, "#75542e", "#75542e"),
    "grass": (3, "#47a638", "#75542e"),
    "sand": (4, "#e0d18f", "#e0d18f"),
    "snow": (5, "#f5f7ff", "#f5f7ff"),
    "water": (6, "#1a8ad1e6", "#1a8ad1e6"),
    "lava": (7, "#e0581a", "#e0581a"),
    "oakTrunk": (8, "#6b4729", "#6b4729"),
    "greenLeaf": (9, "#247029", "#247029"),
    "ironOre": (10, "#a88570", "#a88570"),
    "goldOre": (11, "#dbb23d", "#dbb23d"),
    "crystal": (12, "#9edbf2", "#9edbf2"),
    "obsidian": (13, "#33244c", "#33244c"),
}

# Noms français des blocs que le module ne nomme pas
NOMS = {
    "Sand": "Sable", "Snow": "Neige", "Mud": "Boue", "IronBar": "Bloc de fer", "CopperBar": "Bloc de cuivre",
    "DiamondBar": "Bloc de diamant", "GoldBar": "Bloc d'or", "Glass": "Verre", "Ice": "Glace",
    "MantleStone": "Pierre du manteau", "Lava": "Lave", "Water": "Eau", "Ladder": "Échelle", "Torch": "Torche",
    "YellowFlower": "Fleur jaune", "GlowbellBloom": "Clochelueur en fleur", "Lavender": "Lavande",
    "DeadBush": "Buisson mort", "DarkLeaf": "Feuilles sombres", "RedLeaf": "Feuilles rouges",
    "GreenLeaf": "Feuilles", "LightLeaf": "Feuilles claires", "Glowbell": "Clochelueur", "Tulip": "Tulipe",
    "RedClover": "Trèfle rouge", "RedFlower": "Fleur rouge", "BigBrownShroom": "Grand champignon brun",
    "Iris": "Iris", "Cactus": "Cactus", "Dandelion": "Pissenlit", "RedShroom": "Champignon rouge",
    "BrownShroom": "Champignon brun", "TallGrass1": "Herbes hautes", "TallGrass2": "Herbes hautes",
    "TallGrass3": "Herbes hautes", "Cotton1": "Coton (pousse)", "Cotton2": "Coton (jeune)",
    "Cotton3": "Coton (en croissance)", "Cotton4": "Coton (en boutons)", "Cotton5": "Coton (en fleur)",
    "Cotton6": "Coton (mûr)",
}

# Dossier d'un bloc -> catégorie affichée
CATEGORIES_BLOCS = {
    "mineral": "Minéraux", "soil": "Sols", "flora": "Flore", "tree": "Arbres", "cacti": "Flore",
    "building": "Construction", "bars": "Construction", "liquid": "Liquides", "agriculture": "Cultures",
}

# Sons des modules -> matière des sons de HeroCraft (`audio::synth::Material`)
SONS = {"stone": "stone", "wood": "wood", "leaf": "grass", "grass": "grass", "dirt": "dirt", "sand": "sand",
        "snow": "snow", "gravel": "dirt", "soil": "dirt"}

# Dossier d'un objet -> catégorie affichée
CATEGORIES_OBJETS = {
    "materials": "Matériaux", "tools": "Outils", "weapons": "Armes", "armor": "Armures", "cultures": "Récoltes",
    "plats": "Plats", "food": "Nourriture", "matieres": "Butin", "items": "Totems", "graines": "Graines",
}


def lire_json(chemin):
    """JSON des modules : commentaires `//` et `/* */` permis, hors des chaînes."""
    texte = chemin.read_text(encoding="utf-8")
    sortie, i, n = [], 0, len(texte)
    while i < n:
        c = texte[i]
        if c == '"':
            j = i + 1
            while texte[j] != '"':
                j += 2 if texte[j] == "\\" else 1
            sortie.append(texte[i:j + 1])
            i = j + 1
        elif texte.startswith("//", i):
            i = texte.find("\n", i) if "\n" in texte[i:] else n
        elif texte.startswith("/*", i):
            i = texte.find("*/", i) + 2
        else:
            sortie.append(c)
            i += 1
    return json.loads(re.sub(r",(\s*[}\]])", r"\1", "".join(sortie)))


def ident(nom):
    return nom[0].lower() + nom[1:]


def numeros_existants(dossier):
    """Numéros des fiches déjà écrites, gardés d'une importation à l'autre"""
    numeros = {}
    for fiche in dossier.glob("*.json"):
        try:
            numeros[fiche.stem] = json.loads(fiche.read_text(encoding="utf-8"))["numeric_id"]
        except (ValueError, KeyError):
            pass
    return numeros


def hexa(rgba):
    return "#" + "".join(f"{round(v):02x}" for v in rgba[:3]) + (f"{round(rgba[3]):02x}" if len(rgba) > 3 else "")


def couleur_moyenne(png, teinte=None):
    """Couleur moyenne des texels visibles (alpha ≥ 0,5), multipliée par `teinte` (0-1) au besoin"""
    image = Image.open(png).convert("RGBA")
    octets = image.tobytes()
    texels = [octets[i:i + 4] for i in range(0, len(octets), 4) if octets[i + 3] >= 128]
    if not texels:
        return "#999999"
    moyenne = [sum(p[k] for p in texels) / len(texels) for k in range(3)]
    if teinte:
        moyenne = [v * t for v, t in zip(moyenne, teinte)]
    return hexa(moyenne)


def ecrire(chemin, fiche):
    """Une ligne par champ : facile à relire et à retoucher. Les recettes d'une fiche déjà écrite
    (`tools/importer_recettes.py`, ou à la main) sont gardées."""
    if "recipes" not in fiche and chemin.exists():
        try:
            recettes = json.loads(chemin.read_text(encoding="utf-8")).get("recipes")
        except ValueError:
            recettes = None
        if recettes:
            fiche = {**fiche, "recipes": recettes}
    c = lambda v: json.dumps(v, ensure_ascii=False, separators=(", ", ": "))
    # Les recettes, une par ligne
    v = lambda k, v: "[\n" + ",\n".join(f"    {c(r)}" for r in v) + "\n  ]" if k == "recipes" and v else c(v)
    lignes = ["{"] + [f'  "{k}": {v(k, x)}' + ("," if i < len(fiche) - 1 else "") for i, (k, x) in enumerate(fiche.items())] + ["}"]
    chemin.write_text("\n".join(lignes) + "\n", encoding="utf-8")


class Modules:
    def __init__(self, racine):
        self.racine = pathlib.Path(racine)
        self.tuiles = {}
        for png in self.racine.glob("*/assets/blockTiles/**/*.png"):
            module = png.relative_to(self.racine).parts[0]
            self.tuiles.setdefault(png.stem, {})[module] = png
        self.modeles = {}
        for bloc in self.racine.glob("*/assets/blocks/**/*.block"):
            modele = lire_json(bloc)
            if not modele.get("template") and "tile" not in modele and "tiles" not in modele:
                modele["_nom"] = bloc.stem  # ses textures suivent son nom, et celles des blocs qui en héritent
            self.modeles[f"{bloc.relative_to(self.racine).parts[0]}:{bloc.stem}"] = modele

    def tuile(self, nom, module):
        """Tuile `Module:Nom` ou `Nom`, de préférence dans le module du bloc"""
        if ":" in nom:
            module, nom = nom.split(":")
        choix = self.tuiles.get(nom, {})
        return choix.get(module) or next(iter(choix.values()), None)

    def resoudre(self, definition, module):
        """Le bloc avec ses modèles `basedOn` (un modèle ou un autre bloc), les champs du bloc l'emportant"""
        base = definition.get("basedOn")
        if not base:
            return dict(definition)
        if ":" not in base:
            base = f"{module}:{base}"
        parent = self.resoudre(self.modeles[base], base.split(":")[0])
        parent.pop("template", None)
        return {**parent, **definition}


def importer_blocs(modules):
    BLOCS.mkdir(parents=True, exist_ok=True)
    (BLOCS / "textures").mkdir(exist_ok=True)
    numeros = numeros_existants(BLOCS)
    fiches = []
    for chemin in sorted(modules.racine.glob("*/assets/blocks/**/*.block")):
        if "templates" in chemin.parts:
            continue
        module = chemin.relative_to(modules.racine).parts[0]
        d = modules.resoudre(lire_json(chemin), module)
        nom = chemin.stem
        dossiers = reversed(chemin.parent.parts)
        categorie = next((CATEGORIES_BLOCS[p] for p in dossiers if p in CATEGORIES_BLOCS), None)
        categorie = categorie or ("Ateliers" if module == "HeroCrafting" else "Divers")

        # Textures : `tile` pour toutes les faces, sinon d'après le nom du bloc (ou du bloc dont il
        # hérite) : `NomTop`, `NomSides`… ; puis `tiles`, face par face, l'emporte
        if "tile" in d:
            tuiles = dict.fromkeys(["top", "side", "bottom"], modules.tuile(d["tile"], module))
        else:
            base = d.get("_nom", nom)
            par_nom = lambda *suffixes: next((p for s in suffixes if (p := modules.tuile(base + s, module))), None)
            cote = par_nom("Sides", "", "Front")
            tuiles = {"top": par_nom("Top", "TopBottom", "") or cote, "side": cote,
                      "bottom": par_nom("Bottom", "TopBottom", "") or cote}
        t = d.get("tiles", {})
        for face, cles in (("side", ("sides", "front")), ("top", ("top", "topBottom")), ("bottom", ("bottom", "topBottom"))):
            if cle := next((t[c] for c in cles if c in t), None):
                tuiles[face] = modules.tuile(cle, module)
        tuiles["top"] = tuiles["top"] or tuiles["side"]
        tuiles["bottom"] = tuiles["bottom"] or tuiles["side"]
        if not tuiles.get("side"):
            print(f"Bloc {nom} : aucune texture, ignoré")
            continue

        liquide = bool(d.get("liquid"))
        plante = d.get("shape") == "engine:billboard"
        # Formes de develop reprises en pavés (assets/blocks/shapes/) : la torche a les siennes
        # par face (murale, posée), réduites ici à la torche posée
        forme = "torche" if nom == "Torch" else FORMES.get(d.get("shape"))
        # Herbe et feuillages : texture grise, teintée par le biome dans le jeu d'origine
        teinte = [0.45, 0.78, 0.33] if d.get("colorSource") in ("color_lut", "foliage_lut") else None
        textures = {}
        for face, png in tuiles.items():
            if png is None:
                continue
            cible = BLOCS / "textures" / f"{png.stem}.png"
            shutil.copyfile(png, cible)
            textures[face] = f"textures/{png.stem}.png"
        if textures.get("bottom") == textures.get("side"):
            textures.pop("bottom", None)
        if textures.get("top") == textures.get("side"):
            textures.pop("top", None)

        identifiant = ident(nom)
        dessus, cotes = couleur_moyenne(tuiles["top"] or tuiles["side"], teinte), couleur_moyenne(tuiles["side"], teinte)
        # Verre, glace : faces translucides (les feuillages restent pleins, comme dans le terrain)
        if liquide or d.get("translucent") and not plante and not teinte:
            alpha = "e6" if liquide else "b3"
            dessus, cotes = dessus + alpha, cotes + alpha
        if identifiant in BLOCS_DU_TERRAIN:
            numero, dessus, cotes = BLOCS_DU_TERRAIN[identifiant]
            numeros[identifiant] = numero
        son = (d.get("sounds") or "").split(":")[-1]
        fiche = {
            "numeric_id": numeros.get(identifiant),
            "name": d.get("displayName") or NOMS.get(nom, nom),
            "category": categorie,
            "hardness": d.get("hardness", 3),
            "color": {"top": dessus, "side": cotes} if dessus != cotes else {"side": cotes},
            "textures": textures,
        }
        if teinte:
            fiche["texture_tint"] = hexa([v * 255 for v in teinte])
        if plante:
            fiche["shape"] = "plant"
        elif forme:
            fiche["shape"] = forme
            if d.get("family") == "horizontal":
                fiche["rotation"] = "horizontal"
        if liquide:
            fiche["liquid"] = True
        elif d.get("penetrable") or plante:
            fiche["solid"] = False
        if (d.get("translucent") or liquide) and not plante:
            fiche["opaque"] = False
        if d.get("luminance"):
            fiche["light"] = d["luminance"]
        if nom == "Glass":
            # Verre connecté : un cadre autour de chaque panneau, pas de chaque bloc
            fiche["connected"] = True
        son = "water" if liquide else "grass" if d.get("grass") else SONS.get(son)
        if son and son != "stone":
            fiche["sound"] = son
        if nom == "Grass":
            fiche["drop"] = "dirt"
        elif liquide:
            fiche["drop"] = None
        fiches.append((identifiant, fiche))

    # Numéros des nouveaux blocs, à la suite, dans l'ordre des catégories puis des noms
    suivant = max([n for n in numeros.values() if n is not None and n < PREMIER_OBJET] + [13]) + 1
    for identifiant, fiche in sorted(fiches, key=lambda f: (f[1]["category"], f[0])):
        if fiche["numeric_id"] is None:
            fiche["numeric_id"] = suivant
            suivant += 1
        ecrire(BLOCS / f"{identifiant}.json", fiche)
    print(f"{len(fiches)} blocs écrits dans {BLOCS}")


def atlas_des_modules(racine):
    """`Module:atlas#tuile` -> (image, boîte) pour chaque tuile nommée des atlas"""
    tuiles = {}
    for chemin in racine.glob("*/assets/atlas/*.atlas"):
        module = chemin.relative_to(racine).parts[0]
        a = lire_json(chemin)
        texture = a["texture"].split(":")[-1]
        png = next(racine.glob(f"{module}/assets/textures/**/{texture}.png"), None)
        if png is None:
            continue
        image = Image.open(png).convert("RGBA")
        echelle = image.width / a["textureSize"][0]
        for grille in ([a["grid"]] if "grid" in a else []) + a.get("grids", []):
            (lt, ht), (colonnes, _), (ox, oy) = grille["tileSize"], grille["gridDimensions"], grille.get("gridOffset", [0, 0])
            for i, nom in enumerate(grille["tileNames"]):
                if nom:
                    x, y = ox + (i % colonnes) * lt, oy + (i // colonnes) * ht
                    boite = tuple(round(v * echelle) for v in (x, y, x + lt, y + ht))
                    tuiles[f"{module}:{chemin.stem}#{nom}"] = (image, boite)
    return tuiles


def importer_objets(racine):
    racine = pathlib.Path(racine)
    (OBJETS / "icones").mkdir(parents=True, exist_ok=True)
    numeros = numeros_existants(OBJETS)
    atlas = atlas_des_modules(racine)
    fiches = []
    vus = set()
    for chemin in sorted(racine.glob("*/assets/prefabs/**/*.prefab")):
        p = lire_json(chemin)
        if p.get("parent") != "engine:iconItem":
            continue
        module = chemin.relative_to(racine).parts[0]
        identifiant = ident(chemin.stem)
        if identifiant in vus:
            print(f"Objet {module}:{chemin.stem} : identifiant déjà pris, ignoré")
            continue
        item = p.get("Item", {})
        icone = item.get("icon", "")
        if icone in atlas:
            image, boite = atlas[icone]
            image = image.crop(boite)
        else:
            mod, _, nom = icone.partition(":")
            png = next(racine.glob(f"{mod}/assets/textures/**/{nom}.png"), None)
            if png is None:
                print(f"Objet {identifiant} : icône {icone} introuvable, ignoré")
                continue
            image = Image.open(png).convert("RGBA")
        image.save(OBJETS / "icones" / f"{identifiant}.png")
        vus.add(identifiant)

        dossier = chemin.parent.name
        categorie = CATEGORIES_OBJETS.get(dossier) or CATEGORIES_OBJETS.get(chemin.parent.parent.name, "Divers")
        if "Seed" in p:
            categorie = "Graines"
        stats = {}
        if item.get("baseDamage"):
            stats["Dégâts"] = item["baseDamage"]
        if "Armor" in p:
            stats["Protection"] = p["Armor"].get("protection", 0)
        if "Food" in p:
            stats["Satiété"] = p["Food"].get("nourishment", 0)
        if "Durability" in p:
            stats["Durabilité"] = p["Durability"].get("maxDurability") or p["Durability"].get("durability")
        nom = p.get("DisplayName", {})
        fiche = {
            "numeric_id": numeros.get(identifiant),
            "name": nom.get("name", chemin.stem),
            "description": nom.get("description", ""),
            "category": categorie,
            "max_stack": min(item.get("maxStackSize", 64), 99),
            "slot": p.get("Armor", {}).get("slot"),
            "icon": f"icones/{identifiant}.png",
        }
        if not fiche["slot"]:
            del fiche["slot"]
        if not fiche["description"]:
            del fiche["description"]
        if stats:
            fiche["stats"] = stats
        fiches.append((identifiant, fiche))

    suivant = max([n for n in numeros.values() if n is not None] + [PREMIER_OBJET - 1]) + 1
    for identifiant, fiche in sorted(fiches, key=lambda f: (f[1]["category"], f[0])):
        if fiche["numeric_id"] is None:
            fiche["numeric_id"] = suivant
            suivant += 1
        ecrire(OBJETS / f"{identifiant}.json", fiche)
    print(f"{len(fiches)} objets écrits dans {OBJETS}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    modules = Modules(sys.argv[1])
    importer_blocs(modules)
    importer_objets(modules.racine)
