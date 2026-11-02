#!/usr/bin/env python3
"""Reprend la roue des compétences du carnet Planner (fiche « Classes », #138) dans
`assets/competences/roue.json`.

    PLANNER_TOKEN=... python3 tools/importer_roue_competences.py

Le carnet range les classes par famille (magie, distance, corps à corps) ; chaque classe a ses
niveaux (« Niveau 1 », « Niveau 2 », « Niveau 3 » : les fusions) et son ultime. Les prérequis
sont lus sur la ligne « **Prérequis :** » de chaque compétence, par numéro de fiche (#241).

Sur la roue, chaque classe a sa part du cercle, dans l'ordre du carnet en partant du haut ; les
anneaux vont du niveau 1 (au centre) à l'ultime (au bord). Les compétences d'un anneau sont
rangées sous celles qu'elles demandent, pour que les traits se croisent le moins possible.
"""
import json
import os
import pathlib
import re
import sys
import unicodedata
import urllib.request

RACINE = pathlib.Path(__file__).resolve().parents[1]
SORTIE = RACINE / "assets/competences/roue.json"
API = os.environ.get("PLANNER_API", "https://planner.mathis-medard.fr/api/v1/")
FICHE_CLASSES = 138

# Famille (titre du carnet) -> identifiant et couleur de la roue de référence
FAMILLES = {
    "Classes de magie": ("magic", [124, 88, 178]),
    "Classes de combat à distance": ("ranged", [44, 140, 90]),
    "Classes de corps à corps": ("melee", [192, 72, 52]),
}
RANG_MAX = 3  # #172 : chaque compétence s'améliore jusqu'à trois fois

_cache = {}


def fiche(numero):
    if numero not in _cache:
        requete = urllib.request.Request(API + f"ideas/{numero}")
        jeton = os.environ.get("PLANNER_TOKEN")
        if jeton:
            requete.add_header("Authorization", jeton)
        with urllib.request.urlopen(requete) as reponse:
            _cache[numero] = json.load(reponse)
    return _cache[numero]


def identifiant(titre):
    """« Frappe du Juste » -> « frappeDuJuste » (comme les prefabs de la branche Java)"""
    sans_accents = unicodedata.normalize("NFKD", titre).encode("ascii", "ignore").decode()
    mots = re.findall(r"[A-Za-z0-9]+", sans_accents)
    return mots[0].lower() + "".join(m[:1].upper() + m[1:].lower() for m in mots[1:])


def prerequis(corps):
    for ligne in corps.splitlines():
        if ligne.startswith("**Prérequis"):
            return [int(n) for n in re.findall(r"#(\d+)", ligne)]
    return []


def description(corps):
    """Le corps de la fiche sans ses lignes de liens ni le gras Markdown"""
    lignes = [
        l for l in corps.splitlines()
        if not l.startswith(("**Prérequis", "**Débloque"))
    ]
    texte = "\n".join(lignes).replace("**", "").replace("*", "")
    return re.sub(r"\n{3,}", "\n\n", texte).strip()


def palier(titre_groupe):
    if titre_groupe.startswith(("Niveau 1", "Sorts de base")):
        return 1
    if titre_groupe.startswith("Niveau 2"):
        return 2
    if titre_groupe.startswith("Niveau 3"):
        return 3
    return None


def competence(f, classe, tier, ring=None):
    return {
        "planner": f["id"],
        "name": f["title"],
        "description": description(f["body"]),
        "branch": classe,
        "tier": tier,
        "ring": ring if ring is not None else float(tier),
        "requires": prerequis(f["body"]),
    }


def competences_de(classe_fiche, classe):
    noeuds = []
    for groupe in classe_fiche["children"]:
        g = fiche(groupe["id"])
        tier = palier(g["title"])
        if not g["children"]:
            # Fiche sans enfants au niveau des groupes : l'ultime
            noeuds.append(competence(g, classe, 4))
        elif tier is not None:
            noeuds += [competence(fiche(s["id"]), classe, tier) for s in g["children"]]
        else:
            # Sorcier élémentaire : « Fusion » est elle-même une compétence (niveau 2), qui range
            # sous elle les fusions pures puis mixtes (les fusions d'amplification viennent avec)
            noeuds.append(competence(g, classe, 2))
            for sous in g["children"]:
                s = fiche(sous["id"])
                if "pures" in s["title"]:
                    noeuds += [competence(fiche(c["id"]), classe, 3, 2.5) for c in s["children"]]
                elif "mixtes" in s["title"]:
                    noeuds += [competence(fiche(c["id"]), classe, 3, 3.0) for c in s["children"]]
    return noeuds


def ranger(noeuds):
    """Place chaque compétence dans la part de sa classe : celles d'un anneau sont réparties
    également, dans l'ordre moyen de celles qu'elles demandent"""
    par_numero = {n["planner"]: n for n in noeuds}
    for ring in sorted({n["ring"] for n in noeuds}):
        anneau = [n for n in noeuds if n["ring"] == ring]
        def cle(n, i):
            parents = [par_numero[p]["slot"] for p in n["requires"] if p in par_numero and "slot" in par_numero[p]]
            return (sum(parents) / len(parents)) if parents else i
        ordre = sorted(enumerate(anneau), key=lambda e: cle(e[1], e[0]))
        for rang, (_, n) in enumerate(ordre):
            n["slot"] = round((rang + 0.5) / len(anneau), 4)


def main():
    branches, noeuds = [], []
    for famille in fiche(FICHE_CLASSES)["children"]:
        f = fiche(famille["id"])
        if f["title"] not in FAMILLES:
            continue
        id_famille, couleur = FAMILLES[f["title"]]
        for c in f["children"]:
            classe = fiche(c["id"])
            id_classe = identifiant(classe["title"])
            branches.append({"id": id_classe, "name": classe["title"], "family": id_famille, "color": couleur})
            de_la_classe = competences_de(classe, id_classe)
            ranger(de_la_classe)
            noeuds += de_la_classe
            print(f"{classe['title']} : {len(de_la_classe)} compétences", file=sys.stderr)

    # Identifiants lisibles, uniques sur toute la roue (la classe départage les homonymes)
    noms = {}
    for n in noeuds:
        noms.setdefault(identifiant(n["name"]), []).append(n)
    for base, memes in noms.items():
        for n in memes:
            n["id"] = base if len(memes) == 1 else base + n["branch"][:1].upper() + n["branch"][1:]
    par_numero = {n["planner"]: n["id"] for n in noeuds}
    sortie = []
    for n in noeuds:
        manquants = [p for p in n["requires"] if p not in par_numero]
        if manquants:
            sys.exit(f"{n['name']} : prérequis hors de la roue {manquants}")
        sortie.append({
            "id": n["id"],
            "name": n["name"],
            "description": n["description"],
            "branch": n["branch"],
            "tier": n["tier"],
            "ring": n["ring"],
            "slot": n["slot"],
            "requires": [par_numero[p] for p in n["requires"]],
            "max_rank": RANG_MAX,
            "planner": n["planner"],
        })

    roue = {
        "_source": f"Carnet Planner, fiche #{FICHE_CLASSES} « Classes » ; regénérer avec tools/importer_roue_competences.py",
        "branches": branches,
        "nodes": sortie,
    }
    SORTIE.parent.mkdir(parents=True, exist_ok=True)
    SORTIE.write_text(json.dumps(roue, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"{len(branches)} classes, {len(sortie)} compétences -> {SORTIE.relative_to(RACINE)}", file=sys.stderr)


if __name__ == "__main__":
    main()
