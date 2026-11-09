"""Scènes du Chapelain : le soigneur au sceptre, toute en lumière, chaque prière sa couleur."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater


def soothingLight():
    """Aube rose : le chapelain lève les mains, une petite lumière y naît, des paillettes retombent"""
    t = Toile()
    t.fond_radial(24, 14, 44, [(255, 230, 220), (250, 160, 170), (190, 80, 120), (90, 30, 70), (30, 10, 30)])
    heros(t, "brasLeves", 24, 30, s=1.0, liseres=((OR_PALE, 0, -1),))
    orbe(t, 24, 9, 4, (255, 240, 180))
    sceptre(t, 36, 42, 38, 30)
    for x, y in ((10, 12), (38, 8), (14, 24), (36, 22)):
        t.etincelle(x, y, OR_PALE)
    t.vignette(0.4)
    return t


def protectionAura():
    """Lilas : un allié dans une petite bulle dorée"""
    t = Toile()
    t.fond_radial(24, 26, 40, [(230, 200, 255), (160, 120, 220), (90, 60, 160), (40, 24, 80), (16, 8, 30)])
    bulle = t.masque(lambda d: d.ellipse([9, 8, 39, 42], fill=1))
    t.voile(bulle, (255, 220, 120), 0.28)
    heros(t, "debout", 24, 26, liseres=((OR_PALE, 0, -1),))
    anneau(t, 24, 25, 15, 17, OR, 2, clair=BLANC)
    t.peindre(t.masque(lambda d: d.arc([13, 12, 30, 30], 200, 250, fill=1, width=2)), BLANC)
    t.vignette(0.45)
    return t


def celestialSmite():
    """Ciel d'ardoise : un mince rayon tombe sur l'ennemi, qui garde un voile doré aux pieds"""
    t = Toile()
    t.fond_vertical([(20, 24, 50), (50, 60, 100), (90, 100, 140)], 0, 36)
    sol = t.masque(lambda d: d.rectangle([0, 38, 48, 48], fill=1))
    t.zone_verticale(sol, [(50, 46, 60), (20, 18, 26)], 38, 48)
    t.lueur(26, 30, 14, OR_PALE, 0.5, sy=2.5)
    t.peindre(t.masque(lambda d: d.polygon([(24, 0), (28, 0), (29, 38), (23, 38)], fill=1)), (255, 245, 200))
    t.peindre(t.masque(lambda d: d.line([(26, 0), (26, 38)], fill=1)), BLANC)
    heros(t, "brasOuverts", 26, 27, s=0.95, couleur=(30, 20, 40), liseres=((OR_PALE, 0, -1),))
    anneau(t, 26, 39, 10, 2, OR, 2)
    t.vignette(0.45)
    return t


def blessingOfSpeed():
    """Vert tendre : un allié qui court, des traits dorés tournent autour de ses jambes"""
    t = Toile()
    t.fond_radial(28, 24, 44, [(230, 255, 190), (150, 220, 90), (60, 150, 60), (20, 70, 40), (8, 30, 20)])
    for y, x0 in ((10, 2), (20, 0), (30, 2)):
        t.peindre(t.masque(lambda d, y=y, x0=x0: d.line([(x0, y), (x0 + 10, y)], fill=1)), (230, 255, 200))
    t.peindre(t.masque(lambda d: d.arc([12, 30, 36, 40], 180, 360, fill=1, width=2)), OR)
    heros(t, "course", 25, 24, s=1.05, liseres=((BLANC, 1, 0),))
    t.peindre(t.masque(lambda d: d.arc([12, 30, 36, 40], 20, 160, fill=1, width=2)), OR)
    for x, y in ((8, 36), (40, 34)):
        t.etincelle(x, y, OR_PALE)
    t.vignette(0.4)
    return t


def areaHeal():
    """Prairie d'or : une vague de soin s'étend depuis le chapelain"""
    t = Toile()
    t.fond_vertical([(255, 240, 180), (220, 200, 90), (130, 160, 60)], 0, 30)
    sol = t.masque(lambda d: d.rectangle([0, 30, 48, 48], fill=1))
    t.zone_verticale(sol, [(110, 160, 60), (50, 90, 40), (20, 40, 20)], 30, 48)
    for r in (8, 15, 22):
        anneau(t, 24, 38, r * 1.05, r * 0.32, (255, 230, 120) if r < 20 else (200, 240, 140), 1)
    t.lueur(24, 34, 14, (255, 240, 160), 0.6, sy=0.6)
    heros(t, "brasOuverts", 24, 26, s=0.9, liseres=((BLANC, 0, -1),))
    for x, y in ((8, 34), (40, 35), (14, 42), (34, 43)):
        t.etincelle(x, y, (160, 255, 150))
    t.vignette(0.35)
    return t


def sacredRampart():
    """Indigo : un mur de lumière dressé, les flèches ennemies s'y brisent"""
    t = Toile()
    t.fond_radial(20, 24, 42, [(120, 120, 230), (60, 56, 170), (28, 24, 90), (10, 8, 36)])
    t.lueur(25, 24, 16, (255, 220, 140), 0.55, sy=2)
    mur_m = t.masque(lambda d: d.polygon([(22, 2), (30, 4), (30, 44), (22, 42)], fill=1))
    t.voile(mur_m, (255, 235, 170), 0.6)
    t.peindre(t.masque(lambda d: d.line([(26, 6), (26, 40)], fill=1)), BLANC)
    t.peindre(t.masque(lambda d: d.line([(30, 4), (30, 44)], fill=1, width=2)), OR)
    for y in (12, 24, 35):
        fleche(t, 46, y, 34, y, couleur=(180, 180, 200), fut=(120, 90, 70))
        t.etincelle(32, y, OR_PALE)
    heros(t, "brasTendu", 10, 30, s=0.95, liseres=((OR_PALE, 1, 0),))
    t.vignette(0.45)
    return t


def purification():
    """Blanc et cyan : un éclat sur l'allié, des ombres s'en détachent"""
    t = Toile()
    t.fond_radial(24, 22, 40, [(255, 255, 255), (190, 245, 255), (80, 180, 220), (30, 80, 130), (10, 30, 60)])
    t.rayons(24, 22, 9, 26, 8, BLANC, decalage=22)
    heros(t, "brasOuverts", 24, 26, couleur=(40, 60, 100), liseres=((BLANC, 0, -1),))
    for x, y, a in ((8, 10, 210), (40, 8, -30), (6, 32, 160), (42, 34, 20)):
        m = t.masque(lambda d, x=x, y=y, a=a: d.chord([x - 3, y - 2, x + 3, y + 2], 0, 360, fill=1))
        t.peindre(m, (40, 20, 60))
        t.peindre(t.masque(lambda d, x=x, y=y: d.line([(x, y), (24 + (x - 24) * 0.55, 22 + (y - 22) * 0.55)], fill=1)), (60, 40, 90))
    t.lueur(24, 18, 7, BLANC, 0.9)
    t.vignette(0.35)
    return t


def martyrsMark():
    """Rose de crépuscule : un fil de lumière lie le chapelain à l'allié marqué"""
    t = Toile()
    t.fond_radial(24, 20, 44, [(250, 190, 200), (200, 110, 150), (130, 50, 100), (60, 20, 50), (20, 6, 20)])
    t.peindre(t.masque(lambda d: d.line([(12, 22), (24, 15), (36, 22)], fill=1, width=2)), OR)
    t.peindre(t.masque(lambda d: d.line([(12, 22), (24, 15), (36, 22)], fill=1)), BLANC)
    heros(t, "brasTendu", 10, 30, s=0.8, liseres=((OR_PALE, 1, 0),))
    heros(t, "debout", 37, 30, s=0.8, retourne=True, liseres=((OR_PALE, -1, 0),))
    anneau(t, 37, 39, 8, 2.5, OR, 2, clair=BLANC)
    t.lueur(37, 39, 8, OR, 0.5, sy=0.4)
    t.vignette(0.45)
    return t


def sanctifiedNova():
    """Orange profond : le sceptre levé éclate en une nova dorée"""
    t = Toile()
    t.fond_radial(24, 16, 44, [(255, 240, 180), (250, 170, 60), (210, 90, 30), (110, 30, 20), (40, 10, 10)])
    explosion(t, 24, 14, 16, couleurs=((255, 255, 230), (255, 220, 100), (250, 150, 40)), branches=12)
    anneau(t, 24, 14, 20, 20, (255, 230, 150), 1)
    baton(t, 24, 46, 24, 18, bois=(200, 160, 90), gemme=(255, 250, 220))
    t.vignette(0.4)
    return t


def divineAegis():
    """Bleu acier : un dôme massif se referme sur le chapelain, une onde repousse les ennemis"""
    t = Toile()
    t.fond_radial(24, 30, 42, [(170, 200, 230), (90, 120, 170), (40, 56, 100), (14, 20, 40)])
    sol = t.masque(lambda d: d.rectangle([0, 40, 48, 48], fill=1))
    t.zone_verticale(sol, [(60, 70, 90), (20, 24, 34)], 40, 48)
    dome = t.masque(lambda d: d.pieslice([9, 12, 39, 68], 180, 360, fill=1))
    t.voile(dome, (255, 220, 120), 0.4)
    t.peindre(t.masque(lambda d: d.arc([9, 12, 39, 68], 180, 360, fill=1, width=3)), OR)
    heros(t, "brasLeves", 24, 29, s=0.8, liseres=((OR_PALE, 0, -1),))
    for c in (-1, 1):
        heros(t, "tombe", 24 + c * 19, 36, s=0.55, couleur=(30, 20, 40), liseres=((BLANC, -c, 0),), retourne=c > 0)
    t.vignette(0.4)
    return t


def redemption():
    """Magenta et noir : une large colonne de lumière s'abat, les ennemis pris dans un flash"""
    t = Toile()
    t.fond_radial(24, 30, 40, [(200, 60, 140), (120, 20, 90), (50, 8, 40), (14, 2, 14)])
    col = t.masque(lambda d: d.polygon([(12, 0), (36, 0), (40, 44), (8, 44)], fill=1))
    t.voile(col, (255, 230, 250), 0.45)
    t.peindre(t.masque(lambda d: d.rectangle([21, 0, 27, 44], fill=1)), BLANC)
    t.lueur(24, 42, 18, (255, 220, 240), 0.8, sy=0.4)
    for x in (14, 34):
        heros(t, "brasOuverts", x, 30, s=0.75, couleur=(90, 20, 70), liseres=((BLANC, 0, -1), (BLANC, -1 if x > 24 else 1, 0)))
    t.vignette(0.45)
    return t


def supremeIntervention():
    """Ultime : nuit d'Étherium, des colonnes tombent sur les alliés et les relèvent"""
    t = Toile()
    t.fond_radial(24, 26, 42, [(90, 40, 150), (50, 16, 100), (20, 6, 44), (6, 2, 14)])
    for x, w in ((9, 4), (39, 4), (24, 5)):
        c = t.masque(lambda d, x=x, w=w: d.rectangle([x - w, 0, x + w, 44], fill=1))
        t.voile(c, (200, 150, 255), 0.4)
        t.peindre(t.masque(lambda d, x=x: d.line([(x, 0), (x, 44)], fill=1)), CYAN)
    heros(t, "brasLeves", 24, 28, s=0.9, couleur=(255, 236, 170), liseres=((BLANC, 0, -1), ((230, 160, 60), 0, 1)))
    for x in (9, 39):
        heros(t, "brasOuverts", x, 32, s=0.65, liseres=((CYAN, 0, -1),))
    t.vignette(0.35)
    return t

