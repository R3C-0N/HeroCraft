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
    """Vert de lisière : l'archer bondit en arrière en tirant"""
    t = Toile()
    t.fond_radial(28, 24, 44, [(220, 240, 190), (130, 180, 110), (50, 100, 60), (14, 40, 24)])
    heros(t, "saut", 18, 22, s=0.85, liseres=((BLANC, 1, 0),))
    arc_tir(t, 27, 16, 16)
    fleche(t, 30, 16, 44, 18)
    t.peindre(t.masque(lambda d: d.arc([2, 20, 30, 50], 190, 280, fill=1, width=1)), BLANC)
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
    for x in (20, 30, 40):
        heros(t, "debout", x, 28, s=0.55, couleur=(26, 26, 36), liseres=((BLANC, 0, -1),), retourne=True)
    t.peindre(t.masque(lambda d: d.line([(0, 22), (48, 22)], fill=1, width=3)), (230, 240, 255))
    t.peindre(t.masque(lambda d: d.line([(0, 22), (48, 22)], fill=1)), BLANC)
    t.vignette(0.4)
    return t


def hawkEye():
    """Or de crépuscule : un œil lumineux au-dessus de l'archer concentré"""
    t = Toile()
    t.fond_radial(24, 14, 44, [(255, 240, 180), (230, 170, 80), (150, 80, 40), (60, 26, 16), (20, 8, 6)])
    t.lueur(24, 12, 10, (255, 250, 210), 0.7)
    oeil(t, 24, 12, 8, iris=(240, 170, 40))
    heros(t, "debout", 24, 33, s=0.75, liseres=((OR_PALE, 0, -1),))
    anneau(t, 24, 42, 12, 3, (255, 240, 180), 1)
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
    arc_tir(t, 8, 26, 36)
    fleche(t, 2, 26, 30, 26, epais=2)
    heros(t, "tombe", 38, 24, s=0.7, couleur=(30, 16, 10), liseres=((OR_PALE, 0, -1),))
    for y in (18, 34):
        t.peindre(t.masque(lambda d, y=y: d.line([(14, y), (26, y)], fill=1)), (255, 240, 200))
    t.vignette(0.4)
    return t


def steelDeluge():
    """Acier sous la pluie : la zone disparaît sous une averse de flèches"""
    t = Toile()
    t.fond_vertical([(40, 50, 70), (100, 110, 130), (150, 150, 160)], 0, 40)
    sol = t.masque(lambda d: d.rectangle([0, 40, 48, 48], fill=1))
    t.peindre(sol, (40, 40, 44))
    for i in range(14):
        x, y = 4 + (i * 13) % 42, 8 + (i * 7) % 30
        fleche(t, x + 2, y - 6, x, y, couleur=(220, 225, 235), fut=(160, 160, 170))
    t.vignette(0.4)
    return t


def deadlyHunt():
    """Nuit de traque : un point rouge sur le point faible, du sang à chaque pas"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(90, 110, 90), (40, 56, 44), (16, 24, 18), (4, 8, 6)])
    bete(t, "loup", 22, 26, s=1.05, couleur=(20, 24, 20), liseres=(((200, 220, 180), 0, -1),))
    mire(t, 26, 22, 5, couleur=(255, 50, 40))
    t.point(26, 22, (255, 60, 50))
    sang(t, ((10, 40, 1.5), (18, 42, 1.5), (30, 41, 1.5)))
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
