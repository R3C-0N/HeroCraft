"""Scènes de la Lame du vent : un sabre, et le vent qui coupe."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater

VENT = (210, 250, 240)
VENT_S = (120, 210, 200)


def sabre(t, x0, y0, x1, y1):
    lame(t, x0, y0, x1, y1, metal=(220, 235, 240), poignee=(40, 60, 70), garde=True)


def shearingStrike():
    """Vert d'eau : un grand arc de sabre bordé de souffle pâle"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(220, 255, 240), (110, 200, 180), (30, 110, 110), (8, 40, 44)])
    croissant(t, 24, 26, 19, 190, 350, VENT_S, epais=5)
    croissant(t, 24, 26, 17, 200, 340, VENT, epais=2)
    sabre(t, 12, 40, 34, 16)
    t.vignette(0.4)
    return t


def airBlade():
    """Ciel lavé : le coup détache une lame d'air qui file en avant"""
    t = Toile()
    t.fond_radial(30, 22, 44, [(240, 255, 255), (150, 220, 230), (60, 130, 160), (16, 50, 70)])
    croissant(t, 18, 24, 22, -50, 50, VENT, epais=4)
    for y in (14, 24, 34):
        t.peindre(t.masque(lambda d, y=y: d.line([(4, y), (14, y)], fill=1)), BLANC)
    sabre(t, 4, 42, 18, 30)
    t.vignette(0.4)
    return t


def breezeStep():
    """Vert tendre : une glissade dans une traînée de vent"""
    t = Toile()
    t.fond_radial(30, 26, 44, [(230, 255, 220), (140, 220, 160), (50, 140, 100), (14, 50, 36)])
    for y, l in ((18, 20), (26, 26), (34, 18)):
        t.peindre(t.masque(lambda d, y=y, l=l: d.line([(2, y), (2 + l, y)], fill=1, width=2)), VENT)
    heros(t, "course", 32, 26, liseres=((BLANC, 1, 0),))
    t.vignette(0.4)
    return t


def minorTornado():
    """Turquoise : un coup circulaire du sabre fait naître un petit tourbillon"""
    t = Toile()
    t.fond_radial(28, 26, 40, [(200, 255, 245), (80, 190, 190), (20, 90, 100), (6, 30, 36)])
    sol = t.masque(lambda d: d.rectangle([0, 42, 48, 48], fill=1))
    t.peindre(sol, (10, 40, 44))
    tornade(t, 35, 42, 14, 10, VENT_S, clair=VENT, n=8)
    corps, q = heros(t, "brasTendu", 11, 31, s=0.85, liseres=((VENT, 1, 0),))
    mx, my = q["mainD"]
    sabre(t, mx - 1, my + 1, mx + 9, my - 5)
    croissant(t, mx + 2, my + 4, 9, 250, 340, BLANC, epais=1)
    t.vignette(0.4)
    return t


def piercingGust():
    """Bleu clair : des estocs très rapides, chacun avec un trait d'air"""
    t = Toile()
    t.fond_radial(28, 24, 44, [(230, 245, 255), (130, 180, 230), (50, 90, 160), (14, 26, 60)])
    for y in (14, 22, 30, 38):
        t.peindre(t.masque(lambda d, y=y: d.line([(14 + (y % 3) * 2, y), (46, y)], fill=1, width=1)), VENT)
    sabre(t, 2, 26, 30, 26)
    t.vignette(0.4)
    return t


def windCrescent():
    """Vert sombre : une haute lame d'air verticale traverse plusieurs ennemis"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(180, 240, 220), (60, 150, 130), (20, 70, 60), (6, 24, 20)])
    sol = t.masque(lambda d: d.rectangle([0, 40, 48, 48], fill=1))
    t.peindre(sol, (8, 30, 26))
    for x in (10, 38):
        heros(t, "debout", x, 29, s=0.8, couleur=(14, 36, 30), liseres=((VENT, 0, -1),))
    t.peindre(t.masque(lambda d: d.chord([16, 2, 32, 46], 270, 90, fill=1)), VENT_S)
    t.peindre(t.masque(lambda d: d.chord([20, 2, 30, 46], 270, 90, fill=1)), VENT)
    t.vignette(0.3)
    return t


def zephyrMantle():
    """Vert d'eau lumineux : des spirales de vent tournent autour du corps"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(230, 255, 250), (130, 220, 200), (40, 130, 120), (10, 44, 40)])
    heros(t, "debout", 24, 26, liseres=((BLANC, 0, -1),))
    for i, (y, r) in enumerate(((12, 10), (24, 13), (36, 11))):
        t.peindre(t.masque(lambda d, y=y, r=r, i=i: d.arc([24 - r, y - 3, 24 + r, y + 3], i * 120, i * 120 + 220, fill=1, width=2)), VENT)
    t.vignette(0.4)
    return t


def risingStrike():
    """Bleu de ciel montant : un coup qui part du sol soulève la cible dans une colonne d'air"""
    t = Toile()
    t.fond_vertical([(240, 250, 255), (140, 200, 230), (50, 110, 160)], 0, 48)
    sol = t.masque(lambda d: d.rectangle([0, 42, 48, 48], fill=1))
    t.peindre(sol, (30, 60, 90))
    col = t.masque(lambda d: d.rectangle([24, 0, 42, 42], fill=1))
    t.voile(col, VENT, 0.45)
    for x in (27, 39):
        t.peindre(t.masque(lambda d, x=x: d.line([(x, 38), (x, 26)], fill=1)), BLANC)
    heros(t, "tombe", 33, 12, s=0.75, couleur=(30, 40, 70), liseres=((BLANC, 0, -1),))
    corps, q = heros(t, "frappeHaute", 12, 31, s=0.85, liseres=((VENT, 1, 0),))
    mx, my = q["mainD"]
    sabre(t, mx, my + 2, mx + 8, my - 9)
    croissant(t, 16, 26, 12, 280, 350, BLANC, epais=2)
    t.vignette(0.4)
    return t


def hurricaneDance():
    """Ouragan vert-gris : chaque coup lâche des lames d'air dans toutes les directions"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(220, 240, 230), (120, 170, 160), (50, 80, 80), (14, 24, 24)])
    for a in range(0, 360, 60):
        croissant(t, 24 + 10 * math.cos(math.radians(a)), 24 + 10 * math.sin(math.radians(a)), 10, a - 40, a + 40, VENT, epais=2)
    heros(t, "brasOuverts", 24, 26, s=0.7, liseres=((BLANC, 0, -1),))
    t.vignette(0.4)
    return t


def cuttingTempest():
    """Ardoise et blanc : la lame tourne, puis un croissant horizontal fauche tout autour"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(220, 230, 240), (110, 130, 160), (40, 50, 80), (12, 14, 26)])
    t.peindre(t.masque(lambda d: d.ellipse([2, 22, 46, 34], outline=1, width=3)), VENT_S)
    t.peindre(t.masque(lambda d: d.arc([2, 22, 46, 34], 0, 180, fill=1, width=1)), BLANC)
    heros(t, "garde", 24, 24, s=0.8, liseres=((VENT, 0, -1),))
    t.vignette(0.4)
    return t


def celestialExecution():
    """Ciel d'altitude : la lame du vent s'abat d'en haut et rabat la cible au sol"""
    t = Toile()
    t.fond_vertical([(30, 60, 120), (100, 160, 220), (220, 240, 255)], 0, 48)
    sol = t.masque(lambda d: d.rectangle([0, 42, 48, 48], fill=1))
    t.peindre(sol, (60, 80, 110))
    corps, q = heros(t, "frappeHaute", 15, 20, s=0.7, liseres=((BLANC, 0, -1),))
    mx, my = q["mainD"]
    sabre(t, mx, my + 2, mx + 12, my + 6)
    croissant(t, 22, 22, 12, 300, 420, BLANC, epais=2)
    heros(t, "tombe", 32, 38, s=0.7, couleur=(20, 30, 60), liseres=((BLANC, 0, -1),))
    for x in (22, 42):
        t.peindre(t.masque(lambda d, x=x: d.line([(x, 41), (x + (3 if x > 30 else -3), 36)], fill=1)), BLANC)
    t.vignette(0.35)
    return t


def voidTyphoon():
    """Ultime : le sabre fend l'espace, une tornade d'Étherium noir avance en lacérant"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(110, 60, 180), (54, 20, 100), (20, 6, 44), (6, 2, 14)])
    tornade(t, 28, 44, 4, 16, (40, 10, 60), clair=CYAN, n=9)
    t.peindre(t.masque(lambda d: d.line([(2, 6), (20, 40)], fill=1, width=1)), CYAN)
    sabre(t, 4, 44, 14, 30)
    t.vignette(0.3)
    return t
