"""Scènes de l'Archer : l'arc, la patience et la ligne droite."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater


def chargedShot():
    """Bleu pâle du matin : l'arc s'éclaire pendant la tension"""
    t = Toile()
    t.fond_radial(20, 24, 44, [(250, 250, 240), (180, 210, 230), (90, 130, 170), (30, 50, 80), (10, 18, 30)])
    t.lueur(18, 24, 14, (255, 250, 210), 0.7)
    arc_tir(t, 14, 24, 34)
    fleche(t, 12, 24, 40, 24, couleur=BLANC, epais=1)
    t.lueur(42, 24, 5, BLANC, 0.8)
    t.vignette(0.4)
    return t


def stoppingArrow():
    """Givre bleu : une flèche aux jambes, la cible ralentie garde un voile"""
    t = Toile()
    t.fond_radial(28, 30, 44, [(220, 240, 255), (120, 170, 220), (40, 80, 140), (12, 26, 50)])
    heros(t, "course", 32, 24, s=0.9, couleur=(20, 30, 50), liseres=((BLANC, 0, -1),), retourne=True)
    nuage(t, 32, 36, 18, 6, (200, 230, 255), graine=4, alpha=0.6)
    fleche(t, 4, 34, 24, 34)
    t.vignette(0.4)
    return t


def tacticalRetreat():
    """Vert de lisière : l'archer bondit en arrière en tirant d'instinct"""
    t = Toile()
    t.fond_radial(28, 24, 44, [(220, 240, 190), (130, 180, 110), (50, 100, 60), (14, 40, 24)])
    t.peindre(t.masque(lambda d: d.arc([14, 20, 44, 52], 200, 300, fill=1, width=1)), BLANC)
    corps, q = heros(t, "tir", 16, 24, s=0.9, liseres=((BLANC, 1, 0),))
    mx, my = q["mainG"]
    arc_tir(t, mx + 1, my, 16)
    fleche(t, mx - 3, my, 44, my + 2)
    t.vignette(0.4)
    return t


def multiShot():
    """Ciel d'été : trois flèches partent en éventail"""
    t = Toile()
    t.fond_radial(20, 26, 44, [(250, 250, 220), (150, 200, 240), (60, 110, 190), (20, 40, 90)])
    arc_tir(t, 10, 26, 30)
    for a in (-20, 0, 20):
        r = math.radians(a)
        fleche(t, 10 + 4 * math.cos(r), 26 + 4 * math.sin(r), 10 + 32 * math.cos(r), 26 + 32 * math.sin(r))
    t.vignette(0.4)
    return t


def arrowRain():
    """Gris d'orage : un repère pâle au sol, puis la volée s'abat"""
    t = Toile()
    t.fond_vertical([(60, 60, 80), (120, 120, 140), (160, 150, 150)], 0, 36)
    sol = t.masque(lambda d: d.rectangle([0, 36, 48, 48], fill=1))
    t.zone_verticale(sol, [(80, 80, 70), (30, 30, 26)], 36, 48)
    anneau(t, 24, 40, 16, 4, (240, 240, 220), 1)
    for x, y in ((10, 10), (18, 18), (26, 8), (34, 16), (40, 26), (16, 28), (30, 28)):
        fleche(t, x + 3, y - 8, x, y)
    t.vignette(0.4)
    return t


def piercingArrow():
    """Blanc et ardoise : un trait blanc traverse toute la ligne"""
    t = Toile()
    t.fond_radial(24, 24, 44, [(230, 235, 245), (140, 150, 170), (60, 66, 86), (18, 20, 30)])
    for x in (18, 29, 40):
        heros(t, "debout", x, 30, s=0.7, couleur=(26, 26, 36), liseres=((BLANC, 0, -1),), retourne=True)
    t.peindre(t.masque(lambda d: d.line([(0, 22), (48, 22)], fill=1, width=3)), (230, 240, 255))
    t.peindre(t.masque(lambda d: d.line([(0, 22), (48, 22)], fill=1)), BLANC)
    t.vignette(0.4)
    return t


def hawkEye():
    """Or de crépuscule : l'archer se concentre, un œil lumineux veille au-dessus de lui"""
    t = Toile()
    t.fond_radial(24, 10, 44, [(255, 240, 180), (230, 170, 80), (150, 80, 40), (60, 26, 16), (20, 8, 6)])
    t.lueur(24, 8, 8, (255, 250, 210), 0.7)
    oeil(t, 24, 8, 6, iris=(240, 170, 40))
    corps, q = heros(t, "tir", 22, 33, s=0.9, liseres=((OR_PALE, 0, -1),))
    mx, my = q["mainG"]
    arc_tir(t, mx + 1, my, 14)
    anneau(t, 23, 43, 14, 3, (255, 240, 180), 1)
    t.vignette(0.4)
    return t


def barbedArrow():
    """Rouge sombre : la flèche ouvre une plaie qui saigne"""
    t = Toile()
    t.fond_radial(28, 24, 44, [(240, 170, 150), (170, 60, 60), (90, 16, 26), (30, 4, 8)])
    fleche(t, 4, 30, 32, 18, couleur=(180, 190, 200), epais=2)
    for i in range(3):
        t.peindre(t.masque(lambda d, i=i: d.line([(30 - i * 4, 19 + i * 2), (27 - i * 4, 16 + i * 2)], fill=1)), (180, 190, 200))
    sang(t, ((36, 18, 2.5), (40, 24, 2), (38, 32, 1.5), (42, 38, 1.5)))
    t.vignette(0.4)
    return t


def ballistaShot():
    """Brun de siège : l'arc tendu à l'extrême, la flèche siffle et projette l'ennemi"""
    t = Toile()
    t.fond_radial(30, 22, 44, [(250, 220, 170), (200, 140, 80), (120, 70, 40), (40, 20, 10)])
    arc_tir(t, 8, 24, 24)
    fleche(t, 4, 24, 26, 24, epais=2)
    for y in (18, 30):
        t.peindre(t.masque(lambda d, y=y: d.line([(14, y), (24, y)], fill=1)), (255, 240, 200))
    heros(t, "tombe", 36, 22, s=0.95, couleur=(30, 16, 10), liseres=((OR_PALE, 0, -1),))
    explosion(t, 29, 24, 4, couleurs=(BLANC, (255, 230, 170), (220, 160, 80)), branches=6)
    t.vignette(0.4)
    return t


def steelDeluge():
    """Acier sous la pluie : la zone disparaît sous une averse de flèches"""
    t = Toile()
    t.fond_vertical([(40, 50, 70), (100, 110, 130), (150, 150, 160)], 0, 40)
    sol = t.masque(lambda d: d.rectangle([0, 40, 48, 48], fill=1))
    t.peindre(sol, (40, 40, 44))
    for x, y in ((6, 14), (16, 8), (26, 16), (36, 10), (44, 18), (10, 28), (22, 32), (32, 26), (40, 34), (16, 40), (30, 40)):
        fleche(t, x + 4, y - 11, x, y, couleur=(230, 235, 245), fut=(170, 140, 110), plumes=(220, 60, 50))
    t.vignette(0.4)
    return t


def deadlyHunt():
    """Clairière au crépuscule : un point rouge vise le point faible, du sang à chaque pas"""
    t = Toile()
    t.fond_radial(24, 22, 42, [(200, 220, 170), (110, 140, 90), (44, 64, 40), (12, 20, 12)])
    bete(t, "cerf", 22, 28, s=1.0, couleur=(50, 34, 24), liseres=(((240, 220, 180), 0, -1),))
    mire(t, 24, 25, 5, couleur=(255, 50, 40))
    sang(t, ((10, 42, 1.5), (18, 43, 1.5), (30, 42, 1.5)))
    t.vignette(0.45)
    return t


def etheriumStrike():
    """Ultime : une flèche d'Étherium, un silence, puis un trait cyan qui traverse tout"""
    t = Toile()
    t.fond_radial(20, 24, 42, [(110, 60, 180), (54, 20, 100), (20, 6, 44), (6, 2, 14)])
    t.peindre(t.masque(lambda d: d.line([(14, 24), (48, 24)], fill=1, width=4)), (150, 90, 230))
    t.peindre(t.masque(lambda d: d.line([(14, 24), (48, 24)], fill=1, width=2)), CYAN)
    t.peindre(t.masque(lambda d: d.line([(14, 24), (48, 24)], fill=1)), BLANC)
    arc_tir(t, 10, 24, 32, bois=(60, 30, 80), corde=CYAN)
    cristaux(t, ((30, 14, 5), (38, 34, 5)))
    t.vignette(0.3)
    return t
