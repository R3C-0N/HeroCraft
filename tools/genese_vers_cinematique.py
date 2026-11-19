#!/usr/bin/env python3
"""Change l'export de la maquette HTML de la Genèse (tools/exporter_genese.js) en cinématique de
cartes de papier : assets/cinematics/genese.json et ses découpes dans assets/cinematics/cards/genese/.

Usage : python3 tools/genese_vers_cinematique.py <dossier de l'export>

La caméra reste face à la page, à `DISTANCE` blocs du premier plan. Chaque pièce de papier est posée
à une profondeur qui suit l'ordre où la maquette la dessine (la première au fond), et à la place et
la taille qui la montrent, vue de la caméra, là où la maquette la montre. La caméra glisse un peu à
chaque scène : les plans se décalent les uns devant les autres. Le fond de chaque scène (ciel,
étoiles, lueurs) est une feuille tout au fond, qui change en fondu quand il change.
"""

import json
import math
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageStat

ROOT = Path(__file__).resolve().parent.parent
CARDS = ROOT / "assets/cinematics/cards/genese"
OUT = ROOT / "assets/cinematics/genese.json"

DISTANCE = 30.0  # blocs entre la caméra et le premier plan
FOV = 40.0  # champ vertical, en degrés
DEPTH = 20.0  # profondeur du dernier plan des pièces
BACK = 30.0  # profondeur du fond
DENSITY = 0.6  # pixels de découpe par pixel de la page (1600 × 900)
STEP = 0.5  # pas d'échantillonnage de l'export, en secondes
BACKGROUND_CHANGE = 4.0  # écart moyen (sur 255) au-delà duquel le fond change


def linear(col):
    """`#rrggbb` en rouge, vert, bleu linéaires (None : autre chose qu'une couleur)"""
    if not col or not col.startswith("#") or len(col) != 7:
        return None
    return [((int(col[i : i + 2], 16) / 255) / 12.92) if int(col[i : i + 2], 16) / 255 <= 0.04045 else (((int(col[i : i + 2], 16) / 255) + 0.055) / 1.055) ** 2.4 for i in (1, 3, 5)]


def tint_of(col, reference):
    """Ce qui change la couleur de référence de la découpe en `col`"""
    a, b = linear(col), linear(reference)
    if not a or not b:
        return [1.0, 1.0, 1.0]
    return [round(max(0.0, min(4.0, x / y)), 3) if y > 0.01 else 1.0 for x, y in zip(a, b)]


def unit(z):
    """Taille d'un pixel de la page, en blocs, à la profondeur z"""
    return 2.0 * (z + DISTANCE) * math.tan(math.radians(FOV / 2)) / 900.0


def to_world(cx, cy, w, z):
    u = unit(z)
    return [round((cx - 800.0) * u, 3), round((450.0 - cy) * u, 3), round(z, 3)], round(w * u, 4)


def veil_of(veils):
    """Couleur (#RRGGBB) et opacité des voiles posés l'un sur l'autre"""
    color, alpha = [0.0, 0.0, 0.0], 0.0
    for r, g, b, a, *_ in veils:
        a = max(0.0, min(1.0, a))
        color = [color[0] * (1 - a) * alpha + r * a, color[1] * (1 - a) * alpha + g * a, color[2] * (1 - a) * alpha + b * a]
        alpha = alpha + a * (1 - alpha)
        if alpha > 0:
            color = [c / alpha for c in color]
    return "#%02X%02X%02X" % tuple(round(max(0, min(255, c))) for c in color), alpha


def is_dark(veil):
    """Un voile sombre (la nuit) assombrit ce qu'il couvre ; un voile clair (un éclair) est un fondu"""
    r, g, b = veil[:3]
    return 0.2126 * r + 0.7152 * g + 0.0722 * b < 90


def shade(sample, index):
    """Ce que les voiles sombres posés après la pièce `index` (-1 : le fond) laissent de sa lumière"""
    k = 1.0
    for v in sample.get("veils", []):
        if is_dark(v) and v[4] > index:
            k *= 1.0 - max(0.0, min(1.0, v[3]))
    return k


def simplify(keys, tol_pos, tol_w, tol_a):
    """Garde les étapes qu'une ligne droite entre leurs voisines ne retrouve pas"""
    if len(keys) <= 2:
        return keys
    out = [keys[0]]
    i = 0
    while i < len(keys) - 1:
        j = i + 1
        while j + 1 < len(keys):
            ok = True
            a, b = keys[i], keys[j + 1]
            for m in keys[i + 1 : j + 1]:
                k = (m["t"] - a["t"]) / max(b["t"] - a["t"], 1e-6)
                p = [a["position"][n] + (b["position"][n] - a["position"][n]) * k for n in range(3)]
                tint = [a["tint"][n] + (b["tint"][n] - a["tint"][n]) * k for n in range(3)]
                if max(abs(x - y) for x, y in zip(tint, m["tint"])) > 0.03 or math.dist(p, m["position"]) > tol_pos * unit(m["position"][2]) or abs(a["width"] + (b["width"] - a["width"]) * k - m["width"]) > tol_w * m["width"] or abs(a["opacity"] + (b["opacity"] - a["opacity"]) * k - m["opacity"]) > tol_a:
                    ok = False
                    break
            if not ok:
                break
            j += 1
        out.append(keys[j])
        i = j
    return out


def main():
    src = Path(sys.argv[1] if len(sys.argv) > 1 else "export-genese")
    manifest = json.loads((src / "manifeste.json").read_text())
    S = manifest["S"]
    CARDS.mkdir(parents=True, exist_ok=True)
    for old in CARDS.glob("*.png"):
        old.unlink()

    cards, shots, fades = [], [], []
    previous_veiled_end = False
    for scene in manifest["scenes"]:
        idx, start, end = scene["idx"], scene["start"], scene["end"]
        samples = scene["samples"]

        # Ordre de dessin moyen : la profondeur de chaque pièce
        ranks = {}
        for s in samples:
            n = max(len(s["records"]) - 1, 1)
            for i, r in enumerate(s["records"]):
                ranks.setdefault(r["key"], []).append(i / n)
        order = sorted(ranks, key=lambda k: (sum(ranks[k]) / len(ranks[k]), k))
        depth = {k: DEPTH * (1.0 - i / max(len(order) - 1, 1)) for i, k in enumerate(order)}

        # Le fond, en fondu quand il change
        kept = []
        for bg in scene.get("backgrounds", []):
            image = Image.open(src / "pieces" / bg["file"]).convert("RGB")
            if kept:
                diff = sum(ImageStat.Stat(ImageChops.difference(image, kept[-1][1])).mean) / 3
                if diff < BACKGROUND_CHANGE:
                    continue
            kept.append((bg, image))
        scene_samples = [smp for smp in samples if start <= smp["T"] <= end]
        for j, (bg, image) in enumerate(kept):
            name = f"{idx}-fond-{j}.png"
            image.save(CARDS / name, optimize=True)
            z = BACK - 0.2 * j
            position, width = to_world(800, 450, 1600 * 1.12, z)
            card = {"name": f"Scène {idx + 1}, fond {j + 1}", "image": f"genese/{name}", "at": round(start if j == 0 else kept[j - 1][0]["T"], 3), "duration": None, "position": position, "width": width, "curl": 0, "border": 0}
            card["duration"] = round(end - card["at"], 3)
            # Fondu depuis le fond d'avant, et la nuit qui l'assombrit
            ramp_end = bg["T"] - card["at"]
            keys = []
            for smp in scene_samples:
                t = smp["T"] - card["at"]
                if t < 0:
                    continue
                opacity = 1.0 if j == 0 or ramp_end <= 0 else min(1.0, t / ramp_end)
                k = round(shade(smp, -1), 3)
                keys.append({"t": round(t, 3), "position": position, "width": width, "opacity": round(opacity, 3), "tint": [k, k, k]})
            if j > 0 and keys and keys[0]["t"] > 0:
                keys.insert(0, dict(keys[0], t=0, opacity=0.0))
            keys = simplify(keys, 1.0, 0.01, 0.03)
            if all(all(abs(c - 1.0) < 0.01 for c in key["tint"]) and key["opacity"] > 0.999 for key in keys):
                keys = []
            if keys:
                card["keys"] = keys
            cards.append(card)

        # Les pièces : place à l'écran à chaque échantillon
        best = {}
        for s in samples:
            for r in s["records"]:
                b = r["box"]
                area = (b[2] - b[0]) * (b[3] - b[1]) * (1 if r["alpha"] > 0.05 else 0.001)
                if r["key"] not in best or area > best[r["key"]][0]:
                    best[r["key"]] = (area, b, r.get("col"))
        tracks = {}
        for s in samples:
            for i, r in enumerate(s["records"]):
                tracks.setdefault(r["key"], []).append((s["T"], dict(r, shade=shade(s, i))))
        for key in order:
            img = scene["images"].get(key)
            if not img:
                continue
            ib, rb = img["box"], best[key][1]
            # Découpe réduite à la densité voulue
            source = Image.open(src / "pieces" / img["file"]).convert("RGBA")
            page_w = (ib[2] - ib[0]) / S
            target_w = max(4, min(source.width, round(page_w * DENSITY)))
            if target_w < source.width:
                source = source.resize((target_w, max(4, round(source.height * target_w / source.width))), Image.LANCZOS)
            name = f"{idx}-{key}.png"
            source.save(CARDS / name, optimize=True)

            z = depth[key]
            # Les échantillons d'affilée ; une absence coupe la pièce en deux cartes
            runs, current = [], []
            for T, r in tracks[key]:
                if current and T - current[-1][0] > STEP * 1.5:
                    runs.append(current)
                    current = []
                current.append((T, r))
            runs.append(current)
            for run in runs:
                at = max(start, run[0][0] - STEP / 2)
                stop = min(end, run[-1][0] + STEP / 2)
                keys = []
                for T, r in run:
                    b = r["box"]
                    sx = (b[2] - b[0]) / max(rb[2] - rb[0], 1e-6)
                    sy = (b[3] - b[1]) / max(rb[3] - rb[1], 1e-6)
                    x0, x1 = b[0] + (ib[0] - rb[0]) * sx, b[0] + (ib[2] - rb[0]) * sx
                    y0, y1 = b[1] + (ib[1] - rb[1]) * sy, b[1] + (ib[3] - rb[1]) * sy
                    position, width = to_world((x0 + x1) / 2 / S, (y0 + y1) / 2 / S, (x1 - x0) / S, z)
                    keys.append({"t": round(T - at, 3), "position": position, "width": width, "opacity": round(min(1.0, r["alpha"]), 3), "tint": [round(c * r["shade"], 3) for c in tint_of(r.get("col"), best[key][2])]})
                keys = simplify(keys, 1.0, 0.01, 0.03)
                for k in keys:
                    if all(abs(c - 1.0) < 0.01 for c in k["tint"]):
                        del k["tint"]
                if all(k["opacity"] < 0.02 for k in keys):
                    continue
                card = {"name": f"Scène {idx + 1}, pièce {key}", "image": f"genese/{name}", "at": round(at, 3), "duration": round(max(stop - at, 0.05), 3), "curl": 0, "border": 0}
                blends = {r["blend"] for _, r in run}
                if "lighter" in blends or "screen" in blends:
                    card["glow"] = 1.6
                card["keys"] = keys
                cards.append(card)

        # Une scène, un plan : la caméra glisse doucement, les plans se décalent
        side = 1 if idx % 2 == 0 else -1
        a = [-0.8 * side, 0.0, -DISTANCE]
        b = [0.8 * side, 0.15, -DISTANCE + 1.2]
        shots.append({
            "name": next((p["nom"] for p in reversed(manifest["plans"]) if p["t"] <= start + 0.01), f"Scène {idx + 1}"),
            "duration": round(end - start, 3),
            "position": [a, b],
            "look_at": [[a[0], a[1], 0], [b[0], b[1], 0]],
            "fov": FOV,
            "ease": "in_out",
        })
        # Les voiles de la maquette (la nuit qui tombe, un éclair) : des fondus, d'un échantillon au
        # suivant, fusionnés tant qu'ils vont en ligne droite
        veils = [(smp["T"], veil_of([v for v in smp.get("veils", []) if not is_dark(v)])) for smp in samples]
        segments = []
        for (t0, (c0, a0)), (t1, (c1, a1)) in zip(veils, veils[1:]):
            if a0 < 0.005 and a1 < 0.005:
                continue
            color = c1 if a1 >= a0 else c0
            last = segments[-1] if segments else None
            if last and last["color"] == color and abs(last["at"] + last["duration"] - t0) < 1e-3:
                slope = (last["to"] - last["from"]) / last["duration"]
                if abs(slope * (t1 - t0) - (a1 - a0)) < 0.02 and abs(last["to"] - a0) < 1e-3:
                    last["duration"] = round(t1 - last["at"], 3)
                    last["to"] = round(a1, 3)
                    continue
            segments.append({"at": round(t0, 3), "duration": round(t1 - t0, 3), "from": round(a0, 3), "to": round(a1, 3), "color": color})
        if segments and veils[-1][1][1] < 0.005:
            segments.append({"at": round(veils[-1][0], 3), "duration": 0.01, "from": 0, "to": 0, "color": segments[-1]["color"]})
        # Le voile du début de la scène (aucun, d'ordinaire) : celui de la scène d'avant s'arrête
        if start > 0:
            first = veils[0][1]
            fades.append({"at": round(start, 3), "duration": 0.01, "from": round(first[1], 3), "to": round(first[1], 3), "color": first[0]})
        fades.extend(segments)
        # Les coupes : un bref passage au noir, sauf d'un voile (un éclair blanc) à l'autre
        veiled_start, veiled_end = veils[0][1][1] > 0.8, veils[-1][1][1] > 0.8
        if start > 0 and not veiled_start and not previous_veiled_end:
            fades.append({"at": round(start - 0.25, 3), "duration": 0.25, "from": 0, "to": 1, "color": "#0B0710"})
            fades.append({"at": round(start, 3), "duration": 0.35, "from": 1, "to": 0, "color": "#0B0710"})
        previous_veiled_end = veiled_end

    duration = manifest["scenes"][-1]["end"]
    fades.insert(0, {"at": 0, "duration": 1.2, "from": 1, "to": 0, "color": "#0B0710"})
    fades.append({"at": round(duration - 1.5, 3), "duration": 1.5, "from": 0, "to": 1, "color": "#0B0710"})
    # Le fondu en cours est le dernier commencé
    fades.sort(key=lambda f: (f["at"], f["duration"] > 0.01))
    captions = [
        {"at": t["de"], "duration": round(t["a"] - t["de"], 3), "text": t["t"], "style": "narration", "fade": 0.6}
        for t in manifest["textes"]
        if not t["t"].startswith("Phrase finale")
    ]
    cinematic = {
        "title": "La Genèse",
        "description": "Le récit des origines, porté de la maquette docs/cinematiques/genese.html en cartes de papier découpé : la déchirure, la planète de feu, la pluie des Pierres du Néant, la vie qui naît puis les peuples. Se joue à chaque lancement du jeu (`game_launched`), ou par `/cinematic genese`. Généré par tools/genese_vers_cinematique.py (ne pas modifier à la main).",
        "anchor": "stage",
        # À chaque lancement du jeu (on la passe par Échap, Espace ou Entrée)
        "once": False,
        "triggers": [{"event": "game_launched"}],
        "letterbox": False,
        "stage": {"backdrop": "#0B0710", "light": {"direction": [0.0, 0.3, -1.0], "color": "#FFF6E8", "intensity": 0.55, "ambient": 0.85}},
        "shots": shots,
        "captions": captions,
        "fades": fades,
        "cards": cards,
    }
    text = json.dumps(cinematic, ensure_ascii=False, separators=(",", ":"))
    # Une carte par ligne : lisible et des différences git courtes
    text = text.replace(',"cards":[{', ',\n"cards":[\n{').replace('},{"name"', '},\n{"name"')
    OUT.write_text(text + "\n")
    print(f"{len(cards)} cartes, {len(list(CARDS.glob('*.png')))} découpes, {OUT.stat().st_size // 1024} Ko")


if __name__ == "__main__":
    main()
