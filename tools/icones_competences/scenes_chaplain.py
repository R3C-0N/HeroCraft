"""Scènes du Chapelain : le soigneur au sceptre, toute en lumière, chaque prière sa couleur."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater


def soothingLight():
    """Aube rose : une paume ouverte porte une petite lumière, des paillettes retombent"""
    t = Toile()
    t.fond_radial(24, 20, 44, [(255, 230, 220), (250, 160, 170), (190, 80, 120), (90, 30, 70), (30, 10, 30)])
    orbe(t, 24, 22, 5, (255, 240, 180))
    main = t.masque(lambda d: (d.ellipse([12, 30, 36, 40], fill=1), d.polygon([(10, 33), (6, 28), (9, 26), (14, 31)], fill=1),
                               d.polygon([(38, 33), (42, 28), (39, 26), (34, 31)], fill=1), d.rectangle([18, 38, 30, 47], fill=1)))
    t.objet(main, (236, 186, 150), (255, 225, 195), (170, 110, 90), lumiere=(0, -1))
    for x, y in ((12, 12), (36, 10), (16, 4), (31, 18), (40, 22)):
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
    nuage(t, 24, 4, 44, 10, (70, 76, 110), graine=4)
    t.lueur(26, 30, 14, OR_PALE, 0.5, sy=2.5)
    t.peindre(t.masque(lambda d: d.polygon([(24, 6), (28, 6), (29, 38), (23, 38)], fill=1)), (255, 245, 200))
    t.peindre(t.masque(lambda d: d.line([(26, 6), (26, 38)], fill=1)), BLANC)
    heros(t, "accroupi", 26, 29, liseres=((OR_PALE, 0, -1),))
    anneau(t, 26, 39, 10, 2, OR, 1)
    t.vignette(0.45)
    return t


def blessingOfSpeed():
    """Vert tendre : un allié qui court, des traits dorés tournent autour de ses jambes"""
    t = Toile()
    t.fond_radial(28, 24, 44, [(230, 255, 190), (150, 220, 90), (60, 150, 60), (20, 70, 40), (8, 30, 20)])
    for y, x0 in ((10, 2), (18, 0), (34, 4)):
        t.peindre(t.masque(lambda d, y=y, x0=x0: d.line([(x0, y), (x0 + 12, y)], fill=1)), (230, 255, 200))
    heros(t, "course", 26, 24, liseres=((BLANC, 1, 0),))
    for r, a0 in ((9, 20), (12, 200)):
        t.peindre(t.masque(lambda d, r=r, a0=a0: d.arc([24 - r, 31 - r * 0.35, 24 + r, 31 + r * 0.35], a0, a0 + 140, fill=1, width=2)), OR)
    for x, y in ((12, 28), (38, 30)):
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
    mur_m = t.masque(lambda d: d.rectangle([18, 4, 28, 44], fill=1))
    t.lueur(23, 24, 16, (255, 220, 140), 0.55, sy=2)
    t.voile(mur_m, (255, 230, 150), 0.5)
    for x in (18, 28):
        t.peindre(t.masque(lambda d, x=x: d.line([(x, 4), (x, 44)], fill=1)), OR)
    for y in (10, 20, 30, 40):
        t.peindre(t.masque(lambda d, y=y: d.line([(19, y), (27, y)], fill=1)), OR_PALE)
    for y, l in ((12, 0), (24, 4), (35, 1)):
        fleche(t, 44 - l, y, 31, y, couleur=(180, 180, 200), fut=(120, 90, 70))
        t.etincelle(30, y, OR_PALE)
    heros(t, "brasTendu", 10, 30, s=0.8, liseres=((OR_PALE, 1, 0),))
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
    """Bleu acier : un dôme massif se referme, une onde repousse les ennemis"""
    t = Toile()
    t.fond_radial(24, 30, 42, [(170, 200, 230), (90, 120, 170), (40, 56, 100), (14, 20, 40)])
    sol = t.masque(lambda d: d.rectangle([0, 38, 48, 48], fill=1))
    t.zone_verticale(sol, [(60, 70, 90), (20, 24, 34)], 38, 48)
    dome = t.masque(lambda d: d.pieslice([8, 12, 40, 64], 180, 360, fill=1))
    t.voile(dome, (255, 220, 120), 0.4)
    t.peindre(t.masque(lambda d: d.arc([8, 12, 40, 64], 180, 360, fill=1, width=3)), OR)
    t.peindre(t.masque(lambda d: d.arc([11, 15, 37, 61], 195, 250, fill=1, width=1)), BLANC)
    heros(t, "garde", 24, 27, s=0.8, liseres=((OR_PALE, 0, -1),))
    ondes(t, 24, 30, (230, 240, 255), rayons=(19, 22), debut=-30, fin=10)
    ondes(t, 24, 30, (230, 240, 255), rayons=(19, 22), debut=170, fin=210)
    heros(t, "tombe", 5, 38, s=0.6, couleur=(30, 20, 30), liseres=())
    heros(t, "tombe", 43, 38, s=0.6, retourne=True, couleur=(30, 20, 30), liseres=())
    t.vignette(0.4)
    return t


def redemption():
    """Magenta et noir : une large colonne de lumière s'abat, les ennemis pris dans un flash"""
    t = Toile()
    t.fond_radial(24, 30, 40, [(200, 60, 140), (120, 20, 90), (50, 8, 40), (14, 2, 14)])
    col = t.masque(lambda d: d.polygon([(14, 0), (34, 0), (38, 40), (10, 40)], fill=1))
    t.voile(col, (255, 230, 250), 0.6)
    t.peindre(t.masque(lambda d: d.rectangle([20, 0, 28, 40], fill=1)), BLANC)
    t.lueur(24, 40, 18, (255, 220, 240), 0.9, sy=0.4)
    for x in (14, 34):
        heros(t, "tombe", x, 38, s=0.6, couleur=(70, 20, 60), liseres=((BLANC, 0, -1),), retourne=x > 24)
    t.vignette(0.45)
    return t


def supremeIntervention():
    """Ultime : nuit d'Étherium, des colonnes tombent sur les alliés et les relèvent"""
    t = Toile()
    t.fond_radial(24, 26, 42, [(90, 40, 150), (50, 16, 100), (20, 6, 44), (6, 2, 14)])
    for x, w in ((10, 3), (38, 3), (24, 5)):
        c = t.masque(lambda d, x=x, w=w: d.rectangle([x - w, 0, x + w, 40], fill=1))
        t.voile(c, (200, 150, 255), 0.45)
        t.peindre(t.masque(lambda d, x=x: d.line([(x, 0), (x, 40)], fill=1)), CYAN)
    heros(t, "brasLeves", 24, 28, s=0.85, couleur=(255, 236, 170), liseres=((BLANC, 0, -1), ((230, 160, 60), 0, 1)))
    heros(t, "agenouille", 10, 36, s=0.7, liseres=((CYAN, 0, -1),))
    heros(t, "agenouille", 38, 36, s=0.7, retourne=True, liseres=((CYAN, 0, -1),))
    for x, y in ((6, 10), (42, 12), (16, 20), (32, 18)):
        t.etincelle(x, y, CYAN)
    t.vignette(0.35)
    return t
