#!/usr/bin/env python3
"""Reprend les créatures du module Terasology `HeroBestiary` (branche `develop`) dans
`assets/creatures/` : pour chaque prefab de `assets/prefabs/creatures/`, une description JSON au
format de HeroCraft, plus son modèle glTF et sa texture copiés dans `assets/creatures/modeles/`.

    python3 tools/importer_bestiaire.py CHEMIN/modules/HeroBestiary

Pour partir de la branche `develop` sans la sortir :

    git worktree add /tmp/develop origin/develop
    python3 tools/importer_bestiaire.py /tmp/develop/modules/HeroBestiary

Ce qui est repris : nom et description, boîte de collision (le corps), vitesse de promenade,
habitat (biomes, poids, taille des groupes), maillage, texture et animation. Le reste (prédation,
butin, santé) n'existe pas encore dans HeroCraft. Les biomes sans équivalent (`UNDERGROUND`…) sont
signalés et laissés de côté.
"""
import json
import pathlib
import shutil
import sys

RACINE = pathlib.Path(__file__).resolve().parents[1]
SORTIE = RACINE / "assets/creatures"

# Biomes CoreWorlds -> biomes HeroCraft (`BiomeType`)
BIOMES = {
    "CoreWorlds:PLAINS": ["Plains"],
    "CoreWorlds:FOREST": ["Forest"],
    "CoreWorlds:TAIGA": ["Taiga"],
    "CoreWorlds:MOUNTAINS": ["Mountains"],
    "CoreWorlds:SNOW": ["Tundra", "SnowPeaks"],
    "CoreWorlds:DESERT": ["Desert"],
    "CoreWorlds:BEACH": ["Beach"],
}
# Vitesse de `WanderComponent` quand le prefab ne la donne pas
VITESSE_PAR_DEFAUT = 1.4


def ecrire(chemin, description):
    """Une description par fichier, une ligne par apparition : facile à relire et à retoucher."""
    c = lambda v: json.dumps(v, ensure_ascii=False, separators=(", ", ": "))
    lignes = ["{"]
    elements = list(description.items())
    for i, (cle, valeur) in enumerate(elements):
        fin = "," if i < len(elements) - 1 else ""
        if cle == "spawn" and valeur:
            lignes.append('  "spawn": [')
            lignes += ["    " + c(r) + ("," if j < len(valeur) - 1 else "") for j, r in enumerate(valeur)]
            lignes.append("  ]" + fin)
        else:
            lignes.append(f'  "{cle}": {c(valeur)}{fin}')
    lignes.append("}")
    chemin.write_text("\n".join(lignes) + "\n", encoding="utf-8")


def importer(module):
    module = pathlib.Path(module)
    (SORTIE / "modeles").mkdir(parents=True, exist_ok=True)
    for prefab in sorted((module / "assets/prefabs/creatures").glob("*.prefab")):
        ident = prefab.stem
        p = json.loads(prefab.read_text(encoding="utf-8"))
        maillage = p["SkeletalMesh"]
        nom_maillage = maillage["mesh"].split(":")[-1]
        texture = json.loads((module / "assets/materials" / (maillage["material"].split(":")[-1] + ".mat")).read_text())
        nom_texture = texture["params"]["diffuse"].split(":")[-1]
        shutil.copyfile(module / "assets/skeletalMesh" / f"{nom_maillage}.gltf", SORTIE / "modeles" / f"{ident}.gltf")
        shutil.copyfile(module / "assets/textures" / f"{nom_texture}.png", SORTIE / "modeles" / f"{ident}.png")

        x, y, z = p["BoxShape"]["extents"]
        wander = p.get("Wander")
        habitat = p.get("Habitat", {})
        spawn = []
        for biome in habitat.get("biomes", []):
            if biome not in BIOMES:
                print(f"{ident} : biome {biome} sans équivalent, ignoré")
            for nom in BIOMES.get(biome, []):
                spawn.append({"biome": nom, "weight": habitat.get("densite", 1.0)})
        fiche = SORTIE / f"{ident}.json"
        ancien = json.loads(fiche.read_text(encoding="utf-8")) if fiche.is_file() else {}
        # Le rythme jour/nuit n'existe pas dans le bestiaire : celui déjà choisi est gardé
        activite = ancien.get("activity")
        description = {
            "name": p["DisplayName"]["name"],
            "description": p["DisplayName"].get("description", ""),
            "body": {"half_width": round(x / 2, 4), "half_length": round(z / 2, 4), "height": round(y, 4)},
            "walk_speed": (wander.get("speed", VITESSE_PAR_DEFAUT) if wander else 0.0),
            "min_group": habitat.get("groupeMin", 1),
            "max_group": habitat.get("groupeMax", 1),
            # Le tempérament n'existe pas dans le bestiaire : celui de la fiche actuelle est gardé
            **({"temperament": t} if (t := ancien.get("temperament")) else {}),
            "spawn": spawn,
            **({"activity": activite} if activite else {}),
            "model": {
                "mesh": f"modeles/{ident}.gltf",
                "texture": f"modeles/{ident}.png",
                "walk_animation": maillage.get("animation", "#repos").split("#")[-1],
            },
        }
        ecrire(fiche, description)
        print(f"{ident} : {description['name']}, {len(spawn)} biome(s)")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    importer(sys.argv[1])
