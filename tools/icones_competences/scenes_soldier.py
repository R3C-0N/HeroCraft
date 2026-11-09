"""Scènes du Soldat : l'arme lourde, la poussière et le fracas."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater

POUSSIERE = (200, 180, 140)


def epee_lourde(t, x0, y0, x1, y1):
    lame(t, x0, y0, x1, y1, largeur=3, metal=(200, 205, 215), poignee=(80, 50, 30))


def sol(t, couleurs=((120, 100, 80), (50, 40, 30)), haut=38):
    m = t.masque(lambda d: d.rectangle([0, haut, 48, 48], fill=1))
    t.zone_verticale(m, list(couleurs), haut, 48)
    return m


def heavyStrike():
    """Ocre d'arène : un grand arc lent et un choc qui fait reculer"""
    t = Toile()
    t.fond_radial(28, 24, 44, [(250, 220, 160), (210, 150, 80), (130, 80, 40), (50, 26, 12)])
    croissant(t, 22, 26, 20, 190, 330, (255, 240, 200), epais=4)
    epee_lourde(t, 8, 42, 30, 14)
    explosion(t, 38, 30, 5, couleurs=(BLANC, (255, 230, 170), (220, 160, 80)), branches=7)
    t.vignette(0.4)
    return t


def guardBreaker():
    """Bronze : un coup remontant fait sauter la garde dans des étincelles"""
    t = Toile()
    t.fond_radial(28, 20, 44, [(255, 230, 170), (200, 130, 60), (110, 60, 30), (40, 20, 8)])
    croissant(t, 24, 30, 16, 100, 260, (255, 240, 200), epais=3)
    epee_lourde(t, 14, 44, 30, 12)
    bouclier(t, 36, 10, 6, champ=(120, 40, 40))
    for x, y in ((30, 18), (40, 22), (26, 8)):
        t.etincelle(x, y, (255, 220, 120))
    t.vignette(0.4)
    return t


def frontalRush():
    """Poussière dorée : une charge à l'épaule"""
    t = Toile()
    t.fond_vertical([(150, 170, 190), (220, 200, 150), (200, 160, 100)], 0, 38)
    sol(t)
    nuage(t, 10, 38, 20, 10, POUSSIERE, graine=2)
    heros(t, "course", 26, 26, s=1.05, liseres=((BLANC, 1, 0),))
    t.vignette(0.4)
    return t


def seismicWave():
    """Terre brune : une ligne de débris court devant le soldat"""
    t = Toile()
    t.fond_vertical([(90, 80, 70), (160, 130, 100), (200, 170, 120)], 0, 34)
    sol(t, haut=34)
    for x in range(14, 46, 6):
        h = 4 + (x % 4) * 2
        t.peindre(t.masque(lambda d, x=x, h=h: d.polygon([(x - 3, 40), (x, 40 - h), (x + 3, 40)], fill=1)), (150, 110, 70))
        t.liseré(t.masque(lambda d, x=x, h=h: d.polygon([(x - 3, 40), (x, 40 - h), (x + 3, 40)], fill=1)), (220, 180, 130), -1, -1)
    heros(t, "frappeHaute", 8, 30, s=0.7, liseres=((OR_PALE, 1, 0),))
    t.vignette(0.4)
    return t


def circularSweep():
    """Gris acier : une rotation complète, l'arme tracée en anneau"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(230, 235, 240), (150, 160, 175), (70, 80, 100), (20, 24, 34)])
    t.peindre(t.masque(lambda d: d.ellipse([3, 18, 45, 36], outline=1, width=3)), (255, 250, 230))
    heros(t, "brasOuverts", 24, 24, s=0.85, liseres=((BLANC, 0, -1),))
    epee_lourde(t, 33, 15, 44, 20)
    t.vignette(0.4)
    return t


def rallyingCry():
    """Acier bleuté : une onde sonore part de la bouche, un halo acier autour du soldat"""
    t = Toile()
    t.fond_radial(18, 22, 44, [(220, 230, 245), (130, 150, 180), (60, 70, 100), (18, 22, 34)])
    t.lueur(14, 26, 14, (220, 230, 255), 0.4)
    heros(t, "brasOuverts", 14, 28, s=0.85, liseres=((BLANC, 0, -1),))
    ondes(t, 20, 13, (240, 245, 255), rayons=(7, 13, 19, 25), debut=-35, fin=35)
    t.vignette(0.4)
    return t


def divingStrike():
    """Couchant rouge : un bond, puis un écrasement qui creuse un cratère"""
    t = Toile()
    t.fond_vertical([(70, 30, 50), (200, 80, 60), (250, 170, 100)], 0, 38)
    sol(t)
    cratere = t.masque(lambda d: d.chord([10, 34, 38, 46], 0, 180, fill=1))
    t.peindre(cratere, (40, 24, 20))
    heros(t, "frappeHaute", 24, 18, s=0.85, liseres=((OR_PALE, 0, -1),))
    for x in (8, 40):
        t.peindre(t.masque(lambda d, x=x: d.line([(x, 36), (x + (4 if x < 24 else -4), 30)], fill=1)), POUSSIERE)
    t.vignette(0.4)
    return t


def absoluteParry():
    """Bleu d'acier : une garde haute, chaque coup paré fait jaillir des étincelles"""
    t = Toile()
    t.fond_radial(24, 18, 42, [(220, 230, 250), (110, 130, 180), (40, 50, 90), (10, 14, 30)])
    epee_lourde(t, 6, 30, 42, 14)
    lame(t, 34, 2, 22, 26, metal=(160, 160, 170))
    explosion(t, 26, 20, 5, couleurs=(BLANC, (255, 240, 180), (255, 190, 80)), branches=8)
    for x, y in ((18, 12), (32, 28), (16, 26), (36, 10)):
        t.etincelle(x, y, (255, 220, 120))
    t.vignette(0.4)
    return t


def seismicBreach():
    """Terre ocre : l'arme racle le sol pendant la course, puis projette les gravats"""
    t = Toile()
    t.fond_vertical([(140, 150, 160), (210, 180, 130), (190, 140, 80)], 0, 38)
    sol(t)
    t.peindre(t.masque(lambda d: d.line([(2, 40), (24, 40)], fill=1, width=2)), (255, 200, 120))
    for x, y in ((30, 22), (36, 16), (40, 26), (34, 30)):
        t.peindre(t.masque(lambda d, x=x, y=y: d.ellipse([x - 2, y - 2, x + 2, y + 2], fill=1)), (150, 110, 70))
    heros(t, "course", 18, 26, s=0.9, liseres=((OR_PALE, 1, 0),))
    epee_lourde(t, 26, 24, 28, 40)
    t.vignette(0.4)
    return t


def crushingTornado():
    """Gris et rouille : un saut en vrille, puis l'écrasement au milieu du groupe"""
    t = Toile()
    t.fond_radial(24, 28, 42, [(230, 200, 170), (160, 110, 80), (80, 50, 34), (24, 14, 10)])
    tornade(t, 24, 40, 6, 14, (200, 170, 140), clair=BLANC, n=7)
    heros(t, "brasOuverts", 24, 20, s=0.65, liseres=((BLANC, 0, -1),))
    for x in (6, 42):
        heros(t, "tombe", x, 40, s=0.45, couleur=(40, 24, 20), liseres=(), retourne=x > 24)
    t.vignette(0.4)
    return t


def relentlessExecution():
    """Rouge et acier : l'armure éclate, puis la cible part au loin"""
    t = Toile()
    t.fond_radial(28, 24, 42, [(250, 190, 170), (190, 70, 60), (100, 20, 24), (30, 4, 8)])
    croissant(t, 20, 28, 15, 100, 240, (255, 230, 210), epais=2)
    croissant(t, 20, 22, 15, 250, 360, (255, 230, 210), epais=3)
    for x, y in ((30, 18), (34, 24), (28, 28)):
        t.peindre(t.masque(lambda d, x=x, y=y: d.polygon([(x, y), (x + 3, y - 1), (x + 2, y + 2)], fill=1)), (180, 185, 200))
    heros(t, "tombe", 40, 16, s=0.55, couleur=(30, 10, 14), liseres=((BLANC, 0, -1),))
    epee_lourde(t, 6, 42, 20, 22)
    t.vignette(0.4)
    return t


def voidWarmaster():
    """Ultime : le soldat brille d'Étherium, chaque coup libère une onde qui part très loin"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(110, 60, 180), (54, 20, 100), (20, 6, 44), (6, 2, 14)])
    for r in (12, 17, 22):
        anneau(t, 24, 30, r, r * 0.45, CYAN if r == 17 else (180, 120, 255), 1)
    heros(t, "brasLeves", 24, 30, s=0.9, couleur=(30, 14, 50), liseres=((CYAN, 0, -1), ((180, 120, 255), 0, 1)))
    epee_lourde(t, 33, 12, 40, 2)
    t.vignette(0.3)
    return t
