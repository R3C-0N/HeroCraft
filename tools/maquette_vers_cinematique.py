#!/usr/bin/env python3
"""Change l'export d'une maquette HTML de docs/cinematiques/ (tools/exporter_maquette.js) en
cinématique de cartes de papier : assets/cinematics/<nom>.json et ses découpes dans
assets/cinematics/cards/<nom>/.

Usage : python3 tools/maquette_vers_cinematique.py <nom> <dossier de l'export>
(<nom> : une entrée de `MAQUETTES`, `genese` ou `chronique-du-roi`)

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
# Ce qui ne se lit pas dans la maquette : titre, description, déclencheurs, et les textes encore à
# écrire (leur numéro), qui ne passent pas dans le jeu
MAQUETTES = {
    "genese": {
        "title": "La Genèse",
        "description": "Le récit des origines, porté de la maquette docs/cinematiques/genese.html en cartes de papier découpé : la déchirure, la planète de feu, la pluie des Pierres du Néant, la vie qui naît puis les peuples. Se joue à chaque lancement du jeu (`game_launched`), ou par `/cinematic genese`. Généré par tools/maquette_vers_cinematique.py (ne pas modifier à la main).",
        # À chaque lancement du jeu (on la passe par Échap, Espace ou Entrée)
        "once": False,
        "triggers": [{"event": "game_launched"}],
        "unwritten": [13],  # « Phrase finale ou sous-titre. »
    },
    "chronique-du-roi": {
        "title": "La Chronique du Roi",
        "description": "L'histoire des peuples, la naissance des héros et leur chute, racontée par le Roi (avec ses mensonges), portée de la maquette docs/cinematiques/chronique-du-roi.html en cartes de papier découpé. Se joue sur l'événement `king_chronicle` (quand le Roi la raconte), ou par `/cinematic chronique-du-roi`. Généré par tools/maquette_vers_cinematique.py (ne pas modifier à la main).",
        "once": False,
        "triggers": [{"event": "king_chronicle"}],
        "unwritten": [18],  # « Titre ou dernière phrase. »
    },
}
NAME = sys.argv[1] if len(sys.argv) > 1 else "genese"
CARDS = ROOT / "assets/cinematics/cards" / NAME
OUT = ROOT / f"assets/cinematics/{NAME}.json"

DISTANCE = 30.0  # blocs entre la caméra et le premier plan
FOV = 40.0  # champ vertical, en degrés
DEPTH = 20.0  # profondeur du dernier plan des pièces
BACK = 30.0  # profondeur du fond
DENSITY = 0.6  # pixels de découpe par pixel de la page (1600 × 900)
SHOT_MAX = 4.5  # secondes au plus par plan
# Texels au plus des découpes d'un plan (le jeu en range au plus 6 pages de 2048², marges comprises)
SHOT_TEXELS = 4.5 * 2048 * 2048
VARIANT_BUDGET = 3.0e6  # pixels d'images au plus par pièce animée
STEP = 0.5  # pas d'échantillonnage de l'export, en secondes
BACKGROUND_CHANGE = 1.5  # écart moyen (sur 255) au-delà duquel le fond change


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


GLOW_IMAGE = "lueur.png"
# Le canevas ajoute ses lueurs en sRGB, le jeu en lumière linéaire : plus fort à opacité égale
GLOW_STRENGTH = 0.55


def parse_css(col):
    """`rgba(r,g,b,a)` ou `#rrggbb` en [r, g, b, a] (r, g, b de 0 à 255)"""
    col = (col or "").strip()
    if col.startswith("#") and len(col) == 7:
        return [int(col[i : i + 2], 16) for i in (1, 3, 5)] + [1.0]
    if col.startswith("rgb"):
        v = [float(x) for x in col[col.index("(") + 1 : col.index(")")].split(",")]
        return [round(v[0]), round(v[1]), round(v[2]), v[3] if len(v) > 3 else 1.0]
    return [255, 255, 255, 1.0]


def affine_mul(a, b):
    """a ∘ b, matrices du canevas [a, b, c, d, e, f]"""
    return [a[0] * b[0] + a[2] * b[1], a[1] * b[0] + a[3] * b[1], a[0] * b[2] + a[2] * b[3], a[1] * b[2] + a[3] * b[3], a[0] * b[4] + a[2] * b[5] + a[4], a[1] * b[4] + a[3] * b[5] + a[5]]


def affine_inverse(m):
    det = m[0] * m[3] - m[1] * m[2]
    det = det if abs(det) > 1e-12 else 1e-12
    a, b, c, d = m[3] / det, -m[1] / det, -m[2] / det, m[0] / det
    return [a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5])]


def glow_image(path):
    """La lueur : blanche, d'opacité (1 - r)² comme un dégradé radial ajouté du canevas"""
    n = 256
    im = Image.new("RGBA", (n, n))
    px = im.load()
    for y in range(n):
        for x in range(n):
            r = math.hypot(x + 0.5 - n / 2, y + 0.5 - n / 2) / (n / 2)
            px[x, y] = (255, 255, 255, round(255 * max(0.0, 1.0 - r) ** 2))
    im.save(path, optimize=True)


def save_small(image, path, force=False):
    """PNG à palette (256 couleurs tramées, transparence comprise) : trois fois plus léger, et le
    tramage se fond dans le grain du papier"""
    small = image.quantize(colors=256, method=Image.Quantize.FASTOCTREE, dither=Image.Dither.FLOYDSTEINBERG)
    # Un grand dégradé (la surface de la planète) se verrait en paliers : il garde ses couleurs
    if not force and max(ImageStat.Stat(ImageChops.difference(small.convert("RGBA"), image)).mean[:3]) > 2.5:
        image.save(path, optimize=True)
    else:
        small.save(path, optimize=True)


FINE_IMAGES = {"trait": "trait.png", "trainee": "trainee.png", "point": "point.png"}
FINE_DEPTH = 0.3  # devant les pièces, derrière les textes


def fine_images():
    """Les images des traits et points fins : blanches, teintées par la carte"""
    trait = Image.new("RGBA", (32, 8))
    trainee = Image.new("RGBA", (64, 8))
    for x in range(64):
        for y in range(8):
            edge = max(0.0, 1.0 - abs(y + 0.5 - 4) / 4) ** 0.5
            trainee.putpixel((x, y), (255, 255, 255, round(255 * edge * (1 - x / 63))))
            if x < 32:
                ends = min(1.0, (x + 0.5) / 3, (32 - x - 0.5) / 3)
                trait.putpixel((x, y), (255, 255, 255, round(255 * edge * ends)))
    trait.save(CARDS / FINE_IMAGES["trait"])
    trainee.save(CARDS / FINE_IMAGES["trainee"])
    Image.new("RGBA", (8, 8), (255, 255, 255, 255)).save(CARDS / FINE_IMAGES["point"])


def fine_cards(scene, S, cards):
    """Pluie, cendres, traînées : une carte par trait ou point, tant qu'il glisse sans sauter"""
    tracks = {}
    for smp in scene.get("particles", []):
        for q in smp["prims"]:
            tracks.setdefault(q[0], []).append((smp["T"], q))
    step = 1 / 12
    for pid, track in tracks.items():
        runs, current = [], []
        for T, q in track:
            if current:
                pT, pq = current[-1]
                # Là où il devrait être s'il glissait comme avant ; loin de là, il a sauté (la goutte
                # repart d'en haut)
                guess = pq[2:4] if len(current) < 2 else [2 * a - b for a, b in zip(pq[2:4], current[-2][1][2:4])]
                jump = math.dist(q[2:4], guess) / S
                if T - pT > step * 1.5 or jump > (30 if len(current) >= 2 else 150) or q[1] != pq[1] or q[-3] != pq[-3]:
                    runs.append(current)
                    current = []
            current.append((T, q))
        runs.append(current)
        for run in runs:
            if len(run) < 2:
                continue
            kind, col = run[0][1][1], run[0][1][-3]
            rgba = parse_css(col)
            at = max(scene["start"], run[0][0] - step / 2)
            stop = min(scene["end"], run[-1][0] + step / 2)
            keys = []
            for T, q in run:
                if kind == "point":
                    x, y, w, h = q[2], q[3], q[4], q[5]
                    position, width = to_world(x / S, y / S, max(w, 1) / S, FINE_DEPTH)
                    key = {"t": round(T - at, 3), "position": position, "width": width, "height": round(width * max(h, 1) / max(w, 1), 4), "opacity": round(min(1.0, rgba[3] * q[-2]), 3)}
                else:
                    x0, y0, x1, y1, w = q[2], q[3], q[4], q[5], q[6]
                    length = max(math.hypot(x1 - x0, y1 - y0), 1.0)
                    position, width = to_world((x0 + x1) / 2 / S, (y0 + y1) / 2 / S, length / S, FINE_DEPTH)
                    key = {"t": round(T - at, 3), "position": position, "width": width, "height": round(width * max(w, 1) / length, 4), "opacity": round(min(1.0, rgba[3] * q[-2]), 3), "roll": round(math.degrees(math.atan2(y1 - y0, x1 - x0)), 2)}
                key["tint"] = [1.0, 1.0, 1.0]
                keys.append(key)
            keys = simplify(keys, 1.5, 0.05, 0.05)
            for k in keys:
                del k["tint"]
            card = {"name": f"Scène {scene['idx'] + 1}, {kind} {pid}", "image": f"{NAME}/{FINE_IMAGES[kind]}", "at": round(at, 3), "duration": round(max(stop - at, 0.05), 3), "curl": 0, "border": 0, "blend": "add" if run[0][1][-1] else "alpha", "color": "#%02X%02X%02X" % tuple(rgba[:3]), "keys": keys}
            cards.append(card)


TEAR_DEPTH = -1.0  # devant tout, textes compris
OVERLAY_DEPTH = -1.5  # devant les coupes déchirées


def camera_at(shots, t):
    """L'œil de la caméra à l'instant t (plans en ligne droite, à vitesse constante)"""
    start = 0.0
    for shot in shots:
        if t < start + shot["duration"] or shot is shots[-1]:
            k = min(1.0, max(0.0, (t - start) / shot["duration"]))
            a, b = shot["position"]
            return [a[n] + (b[n] - a[n]) * k for n in range(3)]
        start += shot["duration"]
    return [0.0, 0.0, -DISTANCE]


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
                roll = a.get("roll", 0.0) + (b.get("roll", 0.0) - a.get("roll", 0.0)) * k
                if abs(roll - m.get("roll", 0.0)) > 0.5 or ("height" in m) != ("height" in a) or ("height" in m and abs(a["height"] + (b.get("height", a["height"]) - a["height"]) * k - m["height"]) > 0.01 * m["height"]):
                    ok = False
                    break
                if max(abs(x - y) for x, y in zip(tint, m["tint"])) > 0.03 or math.dist(p, m["position"]) > tol_pos * unit(m["position"][2]) or abs(a["width"] + (b["width"] - a["width"]) * k - m["width"]) > tol_w * m["width"] or abs(a["opacity"] + (b["opacity"] - a["opacity"]) * k - m["opacity"]) > tol_a:
                    ok = False
                    break
            if not ok:
                break
            j += 1
        out.append(keys[j])
        i = j
    return out


def split_heavy_shots(shots, cards):
    """Coupe en deux, le long du même glissement, chaque plan dont les découpes dépassent
    `SHOT_TEXELS` (comme le jeu les range : au plus 1024 pixels, une marge de 16 autour)"""
    sizes = {}

    def texels(name):
        if name not in sizes:
            w, h = Image.open(CARDS.parent / name).size
            k = min(1.0, 1024 / max(w, h))
            sizes[name] = (round(w * k) + 32) * (round(h * k) + 32)
        return sizes[name]

    out, t = [], 0.0
    pending = list(shots)
    while pending:
        shot = pending.pop(0)
        end = t + shot["duration"]
        names = {c["image"] for c in cards if c.get("image") and c["at"] < end and (c["duration"] is None or c["at"] + c["duration"] > t)}
        if sum(texels(n) for n in names) > SHOT_TEXELS and shot["duration"] > 0.5:
            (p0, p1), (l0, l1) = shot["position"], shot["look_at"]
            mid = [round((u + v) / 2, 4) for u, v in zip(p0, p1)]
            look = [round((u + v) / 2, 4) for u, v in zip(l0, l1)]
            half = round(shot["duration"] / 2, 4)
            pending[:0] = [dict(shot, duration=half, position=[p0, mid], look_at=[l0, look]), dict(shot, duration=round(shot["duration"] - half, 4), position=[mid, p1], look_at=[look, l1])]
            continue
        out.append(shot)
        t = end
    return out


def main():
    src = Path(sys.argv[2] if len(sys.argv) > 2 else f"export-{NAME}")
    maquette = MAQUETTES[NAME]
    manifest = json.loads((src / "manifeste.json").read_text())
    S = manifest["S"]
    CARDS.mkdir(parents=True, exist_ok=True)
    for old in CARDS.glob("*.png"):
        old.unlink()
    glow_image(CARDS / GLOW_IMAGE)
    fine_images()

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
            save_small(image.convert('RGBA'), CARDS / name)
            z = BACK - 0.2 * j
            position, width = to_world(800, 450, 1600 * 1.04, z)
            card = {"name": f"Scène {idx + 1}, fond {j + 1}", "image": f"{NAME}/{name}", "at": round(start if j == 0 else kept[j - 1][0]["T"], 3), "duration": None, "position": position, "width": width, "curl": 0, "border": 0}
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

        # Les pièces : place à l'écran à chaque échantillon, par la transformation de la pièce
        # rapportée à celle de l'instant où sa découpe a été dessinée
        best = {}
        for s in samples:
            for r in s["records"]:
                if r.get("glow"):
                    continue
                b = r["box"]
                area = (b[2] - b[0]) * (b[3] - b[1]) * (1 if r["alpha"] > 0.05 else 0.001)
                if r["key"] not in best or area > best[r["key"]][0]:
                    best[r["key"]] = (area, r["m"], r.get("col"))
        tracks = {}
        for s in samples:
            for i, r in enumerate(s["records"]):
                if r.get("covered"):  # une lueur restée dans le fond
                    continue
                tracks.setdefault(r["key"], []).append((s["T"], dict(r, shade=shade(s, i))))
        for key in order:
            if key not in tracks:
                continue
            glow = key.startswith("lueur")
            img = scene["images"].get(key)
            if not img and not glow:
                continue
            caption = key.startswith("31") and len(key.split("_")[0]) == 4
            if caption and int(key[2:4]) in maquette["unwritten"]:  # encore à écrire
                continue
            if glow:
                variants = [(-1e9, GLOW_IMAGE, None)]
            else:
                ib, mb = img["box"], best[key][1]
                # Les images de la pièce (une de plus chaque fois que ce qu'elle porte change),
                # réduites à la densité voulue (les textes restent nets)
                variants = []
                # Au plus `VARIANT_BUDGET` pixels d'images par pièce : une grande pièce change
                # d'image moins souvent (la planète, la rivière de lave)
                texels = ((ib[2] - ib[0]) / S * DENSITY) * ((ib[3] - ib[1]) / S * DENSITY)
                keep = max(2, int(VARIANT_BUDGET / max(texels, 1.0)))
                chosen = img["variants"]
                if len(chosen) > keep:
                    chosen = [chosen[round(i * (len(chosen) - 1) / (keep - 1))] for i in range(keep)]
                for i, v in enumerate(chosen):
                    source = Image.open(src / "pieces" / v["file"]).convert("RGBA")
                    page_w = (ib[2] - ib[0]) / S
                    target_w = max(4, min(source.width, round(page_w * (1.0 if caption else DENSITY))))
                    if target_w < source.width:
                        source = source.resize((target_w, max(4, round(source.height * target_w / source.width))), Image.LANCZOS)
                    name = f"{idx}-{key}-{i}.png"
                    save_small(source, CARDS / name)
                    variants.append((v["T"], name, v.get("col")))
                inverse = affine_inverse(mb)
                centre = ((ib[0] + ib[2]) / 2, (ib[1] + ib[3]) / 2)

            z = depth[key]
            # Les échantillons d'affilée ; une absence coupe la pièce en deux cartes
            runs, current = [], []
            for T, r in tracks[key]:
                if current and T - current[-1][0] > STEP * 1.5:
                    runs.append(current)
                    current = []
                current.append((T, r))
            runs.append(current)
            # Une carte par image : la pièce change d'image d'un échantillon à l'autre
            split = []
            for run in runs:
                part, current_image = [], None
                for T, r in run:
                    image = [(n, col) for vt, n, col in variants if vt <= T + 1e-3][-1:] or [variants[0][1:]]
                    if part and image[0][0] != current_image:
                        split.append((current_image, part))
                        part = []
                    current_image = image[0][0]
                    part.append((T, dict(r, reference=image[0][1])))
                split.append((current_image, part))
            for name, run in split:
                at = max(start, run[0][0] - STEP / 2)
                stop = min(end, run[-1][0] + STEP / 2)
                keys = []
                for T, r in run:
                    if glow:
                        rgba = parse_css(r["col"])
                        position, width = to_world(r["x"] / S, r["y"] / S, 2 * r["r"] / S, z)
                        keys.append({"t": round(T - at, 3), "position": position, "width": width, "opacity": round(min(1.0, GLOW_STRENGTH * rgba[3] * r["alpha"]), 3), "tint": [1.0, 1.0, 1.0]})
                        continue
                    d = affine_mul(r["m"], inverse)
                    cx, cy = d[0] * centre[0] + d[2] * centre[1] + d[4], d[1] * centre[0] + d[3] * centre[1] + d[5]
                    sx, sy = math.hypot(d[0], d[1]), math.hypot(d[2], d[3])
                    position, width = to_world(cx / S, cy / S, (ib[2] - ib[0]) * sx / S, z)
                    key_ = {"t": round(T - at, 3), "position": position, "width": width, "opacity": round(min(1.0, r["alpha"]), 3), "tint": [round(c * r["shade"], 3) for c in tint_of(r.get("col"), r.get("reference"))]}
                    # Sens de la page (y vers le bas) : un angle positif tourne dans le sens des aiguilles
                    key_["roll"] = round(math.degrees(math.atan2(d[1], d[0])), 2)
                    if abs(sy / max(sx, 1e-6) - 1.0) > 0.01:
                        key_["height"] = round(width * (ib[3] - ib[1]) * sy / max((ib[2] - ib[0]) * sx, 1e-6), 4)
                    keys.append(key_)
                keys = simplify(keys, 1.0, 0.01, 0.03)
                for k in keys:
                    if all(abs(c - 1.0) < 0.01 for c in k["tint"]):
                        del k["tint"]
                    if abs(k.get("roll", 0.0)) < 0.05:
                        k.pop("roll", None)
                if all(k["opacity"] < 0.004 for k in keys):
                    continue
                card = {"name": f"Scène {idx + 1}, {'lueur' if glow else 'texte' if caption else 'pièce'} {key}", "image": f"{NAME}/{name}", "at": round(at, 3), "duration": round(max(stop - at, 0.05), 3), "curl": 0, "border": 0, "blend": "alpha"}
                blends = {r.get("blend") for _, r in run}
                if glow:
                    card["blend"] = "add"
                    card["color"] = "#%02X%02X%02X" % tuple(parse_css(run[0][1]["col"])[:3])
                elif "lighter" in blends or "screen" in blends:
                    card["blend"] = "add"
                card["keys"] = keys
                cards.append(card)

        # Une scène, un plan : la caméra glisse doucement, les plans se décalent
        side = 1 if idx % 2 == 0 else -1
        a = [-0.5 * side, 0.0, -DISTANCE]
        b = [0.5 * side, 0.1, -DISTANCE + 1.0]
        fine_cards(scene, S, cards)

        # Plans d'au plus `SHOT_MAX` secondes (les découpes se chargent plan par plan), le long d'un
        # même glissement
        name = next((p["nom"] for p in reversed(manifest["plans"]) if p["t"] <= start + 0.01), f"Scène {idx + 1}")
        parts = max(1, math.ceil((end - start) / SHOT_MAX - 1e-6))
        lerp = lambda u, v, k: [round(u[n] + (v[n] - u[n]) * k, 4) for n in range(3)]
        for part in range(parts):
            p0, p1 = lerp(a, b, part / parts), lerp(a, b, (part + 1) / parts)
            shots.append({
                "name": name if parts == 1 else f"{name} ({part + 1}/{parts})",
                "duration": round((end - start) / parts, 4),
                "position": [p0, p1],
                "look_at": [[p0[0], p0[1], 0], [p1[0], p1[1], 0]],
                "fov": FOV,
                "ease": "linear",
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
        torn = any(abs(t["cut"] - start) < 1e-3 for t in manifest.get("tears", []))
        if start > 0 and not veiled_start and not previous_veiled_end and not torn:
            fades.append({"at": round(start - 0.25, 3), "duration": 0.25, "from": 0, "to": 1, "color": "#0B0710"})
            fades.append({"at": round(start, 3), "duration": 0.35, "from": 1, "to": 0, "color": "#0B0710"})
        previous_veiled_end = veiled_end

    # Les coupes en page déchirée : les images de la maquette, devant tout, centrées sur la caméra
    for tear in manifest.get("tears", []):
        for i, f in enumerate(tear["frames"]):
            name = f"coupe-{tear['cut']}-{i}.png"
            # Une image ne reste à l'écran qu'un douzième de seconde : la palette suffit toujours
            save_small(Image.open(src / "pieces" / f["file"]).convert("RGBA"), CARDS / name, force=True)
            eye = camera_at(shots, f["T"])
            position, width = to_world(800, 450, 1600 * 1.02, TEAR_DEPTH)
            position = [round(position[0] + eye[0], 3), round(position[1] + eye[1], 3), position[2]]
            cards.append({"name": f"Coupe {tear['cut']} s, image {i + 1}", "image": f"{NAME}/{name}", "at": round(f["T"] - 1 / 24, 4), "duration": round(1 / 12, 4), "position": position, "width": width, "curl": 0, "border": 0})

    # Ce que la maquette pose devant toutes les scènes (le cadre doré de la Chronique) : une carte
    # devant tout, qui suit la caméra d'un bout à l'autre de chaque plan
    for i, overlay in enumerate(manifest.get("overlays", [])):
        name = f"devant-{i}.png"
        save_small(Image.open(src / "pieces" / overlay["file"]).convert("RGBA"), CARDS / name, force=True)
        position, width = to_world(800, 450, 1600, OVERLAY_DEPTH)
        times, start = [overlay["from"]], 0.0
        for shot in shots:
            start += shot["duration"]
            if overlay["from"] < start < overlay["to"]:
                times.append(start)
        times.append(overlay["to"])
        keys = []
        for t in times:
            eye = camera_at(shots, t - 1e-4 if t == times[-1] else t)
            keys.append({"t": round(t - overlay["from"], 4), "position": [round(position[0] + eye[0], 3), round(position[1] + eye[1], 3), position[2]], "width": width})
        cards.append({"name": f"Devant {i + 1}", "image": f"{NAME}/{name}", "at": overlay["from"], "duration": round(overlay["to"] - overlay["from"], 4), "curl": 0, "border": 0, "blend": "alpha", "keys": keys})
    shots = split_heavy_shots(shots, cards)
    duration = manifest["scenes"][-1]["end"]
    fades.insert(0, {"at": 0, "duration": 1.2, "from": 1, "to": 0, "color": "#0B0710"})
    fades.append({"at": round(duration - 1.5, 3), "duration": 1.5, "from": 0, "to": 1, "color": "#0B0710"})
    # Le fondu en cours est le dernier commencé
    fades.sort(key=lambda f: (f["at"], f["duration"] > 0.01))
    # Les textes sont des pièces de papier déchiré (cartes « texte »), comme dans la maquette
    captions = []
    cinematic = {
        "title": maquette["title"],
        "description": maquette["description"],
        "anchor": "stage",
        "once": maquette["once"],
        "triggers": maquette["triggers"],
        "letterbox": False,
        "stage": {"backdrop": "#0B0710", "light": {"direction": [0.0, 0.3, -1.0], "color": "#FFF6E8", "intensity": 0.55, "ambient": 0.85}, "vignette": 0.55},
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
