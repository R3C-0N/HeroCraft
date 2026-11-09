#!/usr/bin/env python3
"""Icônes provisoires des compétences : le pictogramme du geste sur la couleur de la classe, avec
dans le coin une pastille pour l'effet le plus parlant (celui qui distingue deux compétences au
même geste). Les ultimes ont un cadre d'Étherium (noir, violet, éclats cyan).

    python3 tools/icones_competences/generer.py                 # toutes les classes
    python3 tools/icones_competences/generer.py warPriest       # une seule
    python3 tools/icones_competences/generer.py --planche=/tmp warPriest  # + planche de relecture
    python3 tools/icones_competences/generer.py --importer effets-visuels-competences.md

La liste (classe, identifiant, geste, effets) est dans `competences.json`, tirée du document
« Effets visuels des 180 compétences » par `--importer`. Une icône dessinée à la main remplace
la provisoire en écrasant `assets/skills/<classe>/<id>.png` : ajoute alors son identifiant à
`retouchees` dans `competences.json` pour que le script ne la regénère plus.
"""
import json
import math
import pathlib
import re
import sys

from PIL import Image, ImageDraw, ImageFilter, ImageFont

RACINE = pathlib.Path(__file__).resolve().parents[2]
DONNEES = pathlib.Path(__file__).with_name("competences.json")
SORTIE = RACINE / "assets/skills"
TAILLE = 128          # taille finale
S = 512               # taille de travail (suréchantillonnage x4)

# Couleurs des classes (document des effets, « Couleurs ») : fond, accent
COULEURS = {
    "warPriest": ((214, 164, 48), (255, 244, 214)),
    "chaplain": ((214, 164, 48), (255, 244, 214)),
    "paladin": ((214, 164, 48), (255, 244, 214)),
    "necromancer": ((112, 62, 150), (190, 235, 170)),
    "druid": ((76, 140, 56), (150, 104, 60)),
    "elementalist": ((70, 110, 200), (220, 236, 255)),
    "thief": ((74, 42, 96), (176, 132, 214)),
    "ranger": ((26, 170, 200), (200, 250, 255)),
    "archer": ((150, 164, 182), (250, 250, 252)),
    "engineer": ((226, 124, 36), (184, 104, 58)),
    "monsterHunter": ((132, 18, 24), (240, 210, 200)),
    "windBlade": ((110, 200, 184), (250, 255, 252)),
    "soldier": ((132, 140, 150), (196, 178, 140)),
    "champion": ((132, 140, 150), (196, 178, 140)),
    "berserker": ((222, 38, 30), (255, 220, 200)),
}
# Éléments du Sorcier élémentaire : la compétence prend la teinte de son élément
ELEMENTS = {
    "feu": (214, 70, 30), "eau": (40, 120, 210), "terre": (130, 96, 56),
    "air": (150, 190, 210), "neant": (90, 40, 140),
}
ETHERIUM = ((16, 10, 26), (124, 58, 200), (90, 240, 255))

# Effets trop courants pour distinguer une compétence d'une autre
COMMUNS = {"impactSpark", "dustPuff", "etheriumAura", "arrowTrail", "slashArc", "weaponGlow"}


# ---------------------------------------------------------------------------------------------
# Outils de dessin (sur un masque en niveaux de gris, à la taille de travail)

def masque():
    m = Image.new("L", (S, S), 0)
    return m, ImageDraw.Draw(m)


def poly_tourne(points, cx, cy, angle):
    a = math.radians(angle)
    c, s = math.cos(a), math.sin(a)
    return [(cx + (x - cx) * c - (y - cy) * s, cy + (x - cx) * s + (y - cy) * c) for x, y in points]


def rect_tourne(d, cx, cy, w, h, angle, rx=None, ry=None, fill=255):
    """Rectangle w×h centré en (rx, ry), tourné de `angle` autour de (cx, cy)"""
    rx = cx if rx is None else rx
    ry = cy if ry is None else ry
    pts = [(rx - w / 2, ry - h / 2), (rx + w / 2, ry - h / 2), (rx + w / 2, ry + h / 2), (rx - w / 2, ry + h / 2)]
    d.polygon(poly_tourne(pts, cx, cy, angle), fill=fill)


def maillet(d, cx, cy, angle, long=230, manche=26, tete=(130, 70)):
    """Un maillet dont le manche part de (cx, cy) ; angle 0 = tête en haut"""
    rect_tourne(d, cx, cy, manche, long, angle, cx, cy - long / 2)
    rect_tourne(d, cx, cy, tete[0], tete[1], angle, cx, cy - long)


def arc_epais(d, boite, debut, fin, ep, fleche=True, fill=255):
    d.arc(boite, debut, fin, fill=fill, width=ep)
    if fleche:
        x0, y0, x1, y1 = boite
        cx, cy, rx, ry = (x0 + x1) / 2, (y0 + y1) / 2, (x1 - x0) / 2 - ep / 2, (y1 - y0) / 2 - ep / 2
        a = math.radians(fin)
        px, py = cx + rx * math.cos(a), cy + ry * math.sin(a)
        # tangente dans le sens du parcours
        tx, ty = -rx * math.sin(a), ry * math.cos(a)
        n = math.hypot(tx, ty)
        tx, ty = tx / n, ty / n
        nx, ny = -ty, tx
        L, W = ep * 2.2, ep * 1.7
        d.polygon([(px + tx * L, py + ty * L), (px + nx * W, py + ny * W), (px - nx * W, py - ny * W)], fill=fill)


def fleche(d, x0, y0, x1, y1, ep, tete=2.4, fill=255):
    dx, dy = x1 - x0, y1 - y0
    n = math.hypot(dx, dy)
    ux, uy = dx / n, dy / n
    L = ep * tete
    d.line([(x0, y0), (x1 - ux * L * 0.8, y1 - uy * L * 0.8)], fill=fill, width=ep)
    nx, ny = -uy, ux
    d.polygon([(x1, y1), (x1 - ux * L + nx * L * 0.7, y1 - uy * L + ny * L * 0.7),
               (x1 - ux * L - nx * L * 0.7, y1 - uy * L - ny * L * 0.7)], fill=fill)


def etoile(d, cx, cy, r, branches=4, creux=0.38, angle=-90, fill=255):
    pts = []
    for i in range(branches * 2):
        a = math.radians(angle + i * 180 / branches)
        rr = r if i % 2 == 0 else r * creux
        pts.append((cx + rr * math.cos(a), cy + rr * math.sin(a)))
    d.polygon(pts, fill=fill)


def main_ouverte(d, cx, cy, k=1.0, fill=255):
    """Une main levée, paume vers l'avant : paume en (cx, cy)"""
    w, h = 120 * k, 110 * k
    d.rounded_rectangle([cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2], radius=40 * k, fill=fill)
    doigts = [(-42, 78, 0), (-14, 96, 0), (14, 92, 0), (42, 76, 0)]
    for dx, L, _ in doigts:
        x = cx + dx * k
        d.rounded_rectangle([x - 13 * k, cy - h / 2 - L * k + 20 * k, x + 13 * k, cy], radius=13 * k, fill=fill)
    # pouce
    rect_tourne(d, cx - w / 2 + 6 * k, cy + 10 * k, 28 * k, 90 * k, -50, cx - w / 2 - 22 * k, cy - 18 * k)


# ---------------------------------------------------------------------------------------------
# Pictogrammes des gestes (assets/hero/animations), centrés sur une zone de 512

def g_overheadStrike(d):
    maillet(d, 300, 430, 28, long=250)
    arc_epais(d, [90, 70, 430, 410], 200, 285, 26)


def g_sweep(d):
    arc_epais(d, [60, 150, 452, 400], 160, 20, 30)
    maillet(d, 250, 440, -70, long=190, tete=(110, 60))


def g_thrust(d):
    d.line([(70, 256), (360, 256)], fill=255, width=30)
    d.polygon([(470, 256), (350, 200), (370, 256), (350, 312)], fill=255)
    for y, x0 in ((176, 110), (336, 110), (216, 60), (296, 60)):
        d.line([(x0, y), (x0 + 120, y)], fill=255, width=16)


def g_groundSlam(d):
    maillet(d, 300, 60, 195, long=260, tete=(150, 80))
    d.rectangle([60, 400, 452, 424], fill=255)
    for a in (-150, -120, -60, -30):
        r = math.radians(a)
        d.line([(256 + 110 * math.cos(r), 380 + 110 * math.sin(r)), (256 + 190 * math.cos(r), 380 + 190 * math.sin(r))],
               fill=255, width=20)


def g_spin(d):
    arc_epais(d, [80, 80, 432, 432], 120, 400, 30)
    rect_tourne(d, 256, 256, 30, 230, 45)
    rect_tourne(d, 256, 256, 90, 22, 45, 256, 290)


def g_charge(d):
    for x in (110, 220, 330):
        d.line([(x, 130), (x + 100, 256), (x, 382)], fill=255, width=46, joint="curve")


def g_guard(d):
    d.polygon([(256, 70), (420, 120), (410, 280), (256, 450), (102, 280), (92, 120)], fill=255)
    d.polygon([(256, 120), (370, 155), (362, 270), (256, 390)], fill=0)


def g_channel(d):
    d.ellipse([196, 196, 316, 316], fill=255)
    for i in range(8):
        a = math.radians(i * 45 + 22)
        d.line([(256 + 110 * math.cos(a), 256 + 110 * math.sin(a)), (256 + 200 * math.cos(a), 256 + 200 * math.sin(a))],
               fill=255, width=22)
        b = math.radians(i * 45 + 22)
        x, y = 256 + 200 * math.cos(b), 256 + 200 * math.sin(b)
        d.ellipse([x - 18, y - 18, x + 18, y + 18], fill=255)


def g_bless(d):
    main_ouverte(d, 256, 330, 1.25)
    for a in (-150, -120, -90, -60, -30):
        r = math.radians(a)
        d.line([(256 + 190 * math.cos(r), 230 + 190 * math.sin(r)), (256 + 240 * math.cos(r), 230 + 240 * math.sin(r))],
               fill=255, width=18)


def g_throw(d):
    for i in range(6):
        t = i / 6
        x, y = 80 + t * 300, 380 - math.sin(t * math.pi * 0.9) * 260
        r = 14 + i * 3
        d.ellipse([x - r, y - r, x + r, y + r], fill=255)
    d.ellipse([360, 160, 460, 260], fill=255)


def g_bowShot(d):
    d.arc([60, 60, 300, 452], -80, 80, fill=255, width=34)
    d.line([(200, 76), (200, 436)], fill=255, width=10)
    fleche(d, 120, 256, 470, 256, 22)


def g_crossbowShot(d):
    rect_tourne(d, 256, 256, 300, 44, 0, 236, 300)          # fût
    d.arc([300, 120, 420, 480], -80, 80, fill=255, width=30)  # arc
    rect_tourne(d, 256, 256, 40, 90, 15, 130, 350)          # crosse
    fleche(d, 160, 236, 480, 236, 18)


def g_shout(d):
    d.polygon([(70, 210), (150, 210), (240, 130), (240, 382), (150, 302), (70, 302)], fill=255)
    for r in (80, 140, 200):
        d.arc([240 - r, 256 - r, 240 + r, 256 + r], -45, 45, fill=255, width=26)


def g_summon(d):
    d.ellipse([70, 360, 442, 450], outline=255, width=26)
    fleche(d, 256, 400, 256, 60, 40, tete=2.2)
    for x, y in ((140, 220), (372, 180), (180, 110)):
        etoile(d, x, y, 34)


def g_leap(d):
    arc_epais(d, [80, 110, 432, 560], 190, 340, 30)
    d.line([(60, 440), (150, 440)], fill=255, width=22)
    d.line([(362, 440), (452, 440)], fill=255, width=22)


def g_aucun(d):
    etoile(d, 256, 256, 190, branches=4, creux=0.32)


GESTES = {k[2:]: v for k, v in globals().items() if k.startswith("g_")}


# ---------------------------------------------------------------------------------------------
# Pastilles des effets (assets/effects), dessinées dans une zone de 512, réduites ensuite

def e_healSparkles(d):
    d.rectangle([206, 90, 306, 422], fill=255)
    d.rectangle([90, 206, 422, 306], fill=255)


def e_handFlame(d):
    d.polygon([(256, 50), (380, 260), (360, 380), (256, 460), (152, 380), (132, 260), (200, 200), (230, 280)], fill=255)


def e_shockRing(d):
    for r in (80, 160, 230):
        d.ellipse([256 - r, 256 - r, 256 + r, 256 + r], outline=255, width=40)


def e_slowFrost(d):
    for i in range(3):
        a = math.radians(i * 60)
        dx, dy = 200 * math.cos(a), 200 * math.sin(a)
        d.line([(256 - dx, 256 - dy), (256 + dx, 256 + dy)], fill=255, width=40)
    for i in range(6):
        a = math.radians(i * 60)
        x, y = 256 + 140 * math.cos(a), 256 + 140 * math.sin(a)
        for s in (-1, 1):
            b = a + s * math.radians(40)
            d.line([(x, y), (x + 60 * math.cos(b), y + 60 * math.sin(b))], fill=255, width=30)


def e_shieldDome(d):
    d.pieslice([60, 120, 452, 532], 180, 360, fill=255)
    d.pieslice([140, 200, 372, 452], 180, 360, fill=0)
    d.rectangle([40, 316, 472, 370], fill=255)


def e_auraRing(d):
    d.ellipse([40, 170, 472, 400], outline=255, width=44)
    etoile(d, 256, 160, 90)


def e_stunStars(d):
    for x, y, r in ((150, 300, 110), (340, 200, 130), (360, 400, 70)):
        etoile(d, x, y, r, branches=5, creux=0.45)


def e_runeCircle(d):
    d.ellipse([40, 40, 472, 472], outline=255, width=36)
    d.polygon([(256, 110), (380, 330), (132, 330)], outline=255, width=30)
    for i in range(6):
        a = math.radians(i * 60 + 30)
        d.ellipse([256 + 216 * math.cos(a) - 26, 256 + 216 * math.sin(a) - 26,
                   256 + 216 * math.cos(a) + 26, 256 + 216 * math.sin(a) + 26], fill=0)


def e_holyBeam(d):
    d.rectangle([206, 0, 306, 380], fill=255)
    for a in (-160, -125, -55, -20):
        r = math.radians(a)
        d.line([(256 + 90 * math.cos(r), 420 + 90 * math.sin(r)), (256 + 200 * math.cos(r), 420 + 200 * math.sin(r))],
               fill=255, width=34)
    d.rectangle([40, 420, 472, 470], fill=255)


def e_weaponGlow(d):
    etoile(d, 256, 256, 220, creux=0.25)


def e_impactSpark(d):
    etoile(d, 256, 256, 220, branches=8, creux=0.45)


def e_groundBurst(d):
    d.rectangle([40, 400, 472, 450], fill=255)
    for x, y, r in ((150, 300, 70), (300, 220, 90), (400, 330, 50), (220, 130, 40)):
        d.regular_polygon((x, y, r), 5, rotation=x, fill=255)


def e_dustPuff(d):
    for x, y, r in ((160, 300, 100), (290, 260, 130), (380, 330, 80)):
        d.ellipse([x - r, y - r, x + r, y + r], fill=255)


EFFETS = {k[2:]: v for k, v in globals().items() if k.startswith("e_")}


# ---------------------------------------------------------------------------------------------
# Composition

def melange(a, b, t):
    return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))


def fond(base, ultime):
    """Carré arrondi : dégradé radial de la couleur de la classe"""
    img = Image.new("RGBA", (S, S))
    clair, sombre = melange(base, (255, 255, 255), 0.25), melange(base, (0, 0, 0), 0.55)
    px = img.load()
    for y in range(S):
        for x in range(S):
            r = math.hypot(x - S * 0.42, y - S * 0.36) / (S * 0.8)
            px[x, y] = melange(clair, sombre, min(1, r)) + (255,)
    forme = Image.new("L", (S, S), 0)
    ImageDraw.Draw(forme).rounded_rectangle([8, 8, S - 8, S - 8], radius=70, fill=255)
    img.putalpha(forme)
    return img


def cadre(img, base, accent, ultime):
    d = ImageDraw.Draw(img)
    if ultime:
        noir, violet, cyan = ETHERIUM
        d.rounded_rectangle([8, 8, S - 8, S - 8], radius=70, outline=noir + (255,), width=40)
        d.rounded_rectangle([28, 28, S - 28, S - 28], radius=56, outline=violet + (255,), width=14)
        for x, y in ((40, 40), (S - 40, 40), (40, S - 40), (S - 40, S - 40)):
            etoile(d, x, y, 38, creux=0.25, fill=cyan + (255,))
    else:
        d.rounded_rectangle([8, 8, S - 8, S - 8], radius=70, outline=melange(base, (0, 0, 0), 0.7) + (255,), width=18)
        d.rounded_rectangle([26, 26, S - 26, S - 26], radius=56, outline=accent + (150,), width=6)


def appliquer(img, m, couleur, contour, ep):
    """Pose le masque `m` en `couleur` avec un contour sombre de `ep` pixels"""
    if ep:
        bord = m.filter(ImageFilter.MaxFilter(ep * 2 + 1))
        img.paste(Image.new("RGBA", (S, S), contour + (255,)), (0, 0), bord)
    img.paste(Image.new("RGBA", (S, S), couleur + (255,)), (0, 0), m)


def reduire(m, k, cx, cy):
    """Réduit un masque de 512 d'un facteur k et le centre en (cx, cy)"""
    petit = m.resize((round(S * k), round(S * k)), Image.LANCZOS)
    out = Image.new("L", (S, S), 0)
    out.paste(petit, (round(cx - S * k / 2), round(cy - S * k / 2)))
    return out


def badge_effet(competence):
    """L'effet qui distingue la compétence : le plus rare parmi les siens, hors effets courants"""
    if competence.get("badge"):
        return competence["badge"]
    candidats = [e for e in competence["effects"] if e in EFFETS and e not in COMMUNS]
    if not candidats:
        candidats = [e for e in competence["effects"] if e in EFFETS and e != "etheriumAura"]
    if not candidats:
        return None
    return min(candidats, key=lambda e: (FREQUENCES.get(e, 0), competence["effects"].index(e)))


def icone(competence):
    classe = competence["class"]
    base, accent = COULEURS[classe]
    if competence.get("element"):
        base = ELEMENTS[competence["element"]]
    ultime = competence.get("ultimate", False)
    img = fond(base, ultime)
    sombre = melange(base, (0, 0, 0), 0.75)

    m, d = masque()
    GESTES.get(competence["gesture"], g_aucun)(d)
    effet = badge_effet(competence)
    k = 0.72 if effet else 0.82
    appliquer(img, reduire(m, k, S * 0.47, S * 0.47), (255, 255, 255), sombre, 14)

    if effet:
        cx, cy, r = S * 0.745, S * 0.745, S * 0.17
        dd = ImageDraw.Draw(img)
        dd.ellipse([cx - r - 10, cy - r - 10, cx + r + 10, cy + r + 10], fill=sombre + (255,))
        dd.ellipse([cx - r, cy - r, cx + r, cy + r], fill=melange(base, (0, 0, 0), 0.4) + (255,))
        me, de = masque()
        EFFETS[effet](de)
        appliquer(img, reduire(me, 0.26, cx, cy), accent, sombre, 0)

    cadre(img, base, accent, ultime)
    return img.resize((TAILLE, TAILLE), Image.LANCZOS)


# ---------------------------------------------------------------------------------------------
# Liste des compétences

def importer(md):
    """Tire du document des effets la classe, l'identifiant, le geste et les effets"""
    roue = json.loads((RACINE / "assets/competences/roue.json").read_text())
    rang = {n["name"]: n["tier"] for n in roue["nodes"]}
    ultime_max = max(rang.values())
    competences, classe, element = [], None, None
    elements = {"Feu": "feu", "Eau": "eau", "Terre": "terre", "Air": "air"}
    for ligne in pathlib.Path(md).read_text().splitlines():
        m = re.match(r"## .+? \(`(\w+)`\)", ligne)
        if m:
            classe, element = m.group(1), None
            continue
        if ligne.startswith("## "):
            classe = None
        if not classe or not ligne.startswith("| ") or ligne.startswith("| Compétence"):
            continue
        cols = [c.strip() for c in ligne.split("|")[1:-1]]
        if len(cols) < 4:
            continue
        m = re.match(r"(.+?) `(\w+)`", cols[0])
        if not m:
            continue          # second sort d'un élément (« Feu · Rune Volcanique ») : même icône
        nom, ident = m.group(1).strip(), m.group(2)
        if classe == "elementalist":
            element = elements.get(nom.split(" ")[0], "neant" if "Néant" in nom else None)
        geste = re.match(r"`(\w+)`", cols[1])
        competences.append({
            "class": classe, "id": ident, "name": nom,
            "gesture": geste.group(1) if geste else "aucun",
            "effects": re.findall(r"★?`(\w+)`", cols[2]),
            "ultimate": rang.get(nom) == ultime_max or "etheriumAura" in cols[2],
            **({"element": element} if classe == "elementalist" and element else {}),
        })
    ancien = json.loads(DONNEES.read_text()) if DONNEES.exists() else {}
    DONNEES.write_text(json.dumps({
        "_source": "Effets visuels des 180 compétences ; regénérer avec generer.py --importer",
        "retouchees": ancien.get("retouchees", []),
        "skills": competences,
    }, ensure_ascii=False, indent=1) + "\n")
    print(f"{len(competences)} compétences dans {DONNEES.relative_to(RACINE)}")


def planche(competences, chemin):
    """Toutes les icônes d'une classe côte à côte, avec leur nom et leur geste"""
    col, case = 4, 220
    lignes = math.ceil(len(competences) / col)
    img = Image.new("RGB", (col * case, lignes * case + 10), (30, 30, 36))
    d = ImageDraw.Draw(img)
    try:
        police = ImageFont.truetype("DejaVuSans.ttf", 14)
    except OSError:
        police = ImageFont.load_default()
    for i, c in enumerate(competences):
        x, y = (i % col) * case, (i // col) * case
        ic = Image.open(SORTIE / c["class"] / f"{c['id']}.png")
        img.paste(ic, (x + (case - TAILLE) // 2, y + 14), ic)
        for j, texte in enumerate((c["name"], f"{c['gesture']} · {badge_effet(c) or '—'}")):
            w = d.textlength(texte, font=police)
            d.text((x + (case - w) / 2, y + 152 + j * 20), texte, fill=(235, 235, 235) if j == 0 else (150, 150, 160), font=police)
    img.save(chemin)
    print(f"planche : {chemin}")


def main(args):
    if args[:1] == ["--importer"]:
        importer(args[1])
        return
    planche_dans = next((a.split("=", 1)[1] for a in args if a.startswith("--planche=")), None)
    classes = [a for a in args if not a.startswith("--")]
    donnees = json.loads(DONNEES.read_text())
    toutes = donnees["skills"]
    global FREQUENCES
    FREQUENCES = {}
    for c in toutes:
        for e in set(c["effects"]):
            FREQUENCES[e] = FREQUENCES.get(e, 0) + 1
    choisies = [c for c in toutes if not classes or c["class"] in classes]
    for c in choisies:
        if c["id"] in donnees["retouchees"]:
            continue
        dossier = SORTIE / c["class"]
        dossier.mkdir(parents=True, exist_ok=True)
        icone(c).save(dossier / f"{c['id']}.png", optimize=True)
    print(f"{len(choisies)} icônes dans {SORTIE.relative_to(RACINE)}")
    if planche_dans:
        for classe in classes or sorted({c["class"] for c in choisies}):
            planche([c for c in choisies if c["class"] == classe], pathlib.Path(planche_dans) / f"planche-{classe}.png")


FREQUENCES = {}

if __name__ == "__main__":
    main(sys.argv[1:])
