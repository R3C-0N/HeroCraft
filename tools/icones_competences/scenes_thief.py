"""Scènes du Voleur : arbalète de poing, ombre et poison, toujours un peu dans la nuit."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater

OMBRE_V = (60, 30, 80)
POISON = (130, 230, 60)


def fumee(t, cx, cy, w, h, graine=1, couleur=(50, 30, 70)):
    return nuage(t, cx, cy, w, h, couleur, sombre=(24, 12, 34), graine=graine)


def quickShot():
    """Nuit bleu acier : un carreau très rapide avec une traînée courte"""
    t = Toile()
    t.fond_radial(30, 22, 44, [(170, 190, 230), (80, 100, 160), (30, 40, 80), (8, 12, 30)])
    for y in (16, 28):
        t.peindre(t.masque(lambda d, y=y: d.line([(10, y), (24, y)], fill=1)), (200, 210, 240))
    fleche(t, 14, 22, 40, 22, trainee=(220, 230, 255), plumes=(160, 60, 200), epais=2)
    arbalete(t, 2, 34, couleur=(70, 50, 60))
    t.vignette(0.4)
    return t


def poisonBolt():
    """Vert de venin : un carreau à traînée verte, la cible fume de vapeur toxique"""
    t = Toile()
    t.fond_radial(30, 28, 44, [(180, 230, 120), (80, 150, 50), (30, 70, 24), (8, 24, 8)])
    nuage(t, 36, 18, 14, 10, POISON, graine=4, alpha=0.6)
    heros(t, "accroupi", 36, 30, s=0.8, couleur=(30, 40, 20), liseres=((POISON, 0, -1),), retourne=True)
    fleche(t, 6, 26, 26, 26, couleur=POISON, trainee=(160, 255, 90), fut=(80, 60, 50))
    t.vignette(0.45)
    return t


def shadowVeil():
    """Violet d'encre : une bouffée de fumée d'ombre, le voleur disparaît"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(150, 100, 190), (80, 40, 120), (34, 14, 56), (10, 4, 18)])
    heros(t, "debout", 24, 26, couleur=(40, 20, 60), liseres=(((180, 120, 230), 0, -1),))
    fumee(t, 24, 34, 40, 16, graine=3)
    fumee(t, 14, 16, 14, 10, graine=5)
    fumee(t, 36, 20, 12, 8, graine=6)
    t.vignette(0.45)
    return t


def shadowDecoy():
    """Violet et noir : un double d'ombre se lève dans la fumée"""
    t = Toile()
    t.fond_radial(24, 22, 40, [(180, 130, 220), (100, 50, 150), (40, 16, 70), (12, 4, 20)])
    heros(t, "debout", 32, 26, couleur=(20, 10, 30), liseres=(((200, 150, 255), 0, -1),))
    m, _ = heros(t, "debout", 16, 26, couleur=(90, 50, 130), liseres=())
    t.voile(m, (160, 110, 210), 0.4)
    fumee(t, 16, 40, 24, 10, graine=2)
    t.vignette(0.45)
    return t


def shardShot():
    """Gris d'orage : le carreau éclate en une gerbe de fragments"""
    t = Toile()
    t.fond_radial(32, 22, 44, [(220, 220, 230), (130, 130, 150), (60, 60, 80), (20, 20, 30)])
    fleche(t, 4, 30, 24, 24, epais=2)
    explosion(t, 30, 22, 6, couleurs=(BLANC, (220, 230, 255), (150, 160, 190)), branches=7)
    for a in (-60, -30, 0, 30, 60):
        r = math.radians(a)
        x, y = 30 + 14 * math.cos(r), 22 + 14 * math.sin(r)
        t.peindre(t.masque(lambda d, x=x, y=y: d.line([(30 + 8 * math.cos(r), 22 + 8 * math.sin(r)), (x, y)], fill=1)), (230, 235, 250))
    t.vignette(0.4)
    return t


def grappleBolt():
    """Toits au crépuscule : un câble tendu, le voleur s'y fait tirer"""
    t = Toile()
    t.fond_vertical([(40, 30, 70), (150, 80, 110), (240, 150, 100)], 0, 48)
    toit = t.masque(lambda d: d.polygon([(30, 18), (48, 10), (48, 48), (30, 48)], fill=1))
    t.peindre(toit, (24, 16, 30))
    t.peindre(t.masque(lambda d: d.line([(8, 36), (36, 14)], fill=1)), (230, 220, 200))
    fleche(t, 32, 17, 37, 13, fut=(200, 200, 200))
    heros(t, "saut", 10, 34, s=0.75, couleur=(30, 16, 40), liseres=(((255, 180, 140), 1, 0),))
    t.vignette(0.4)
    return t


def ambush():
    """Noir et éclat violet : le voleur surgit de la fumée et frappe"""
    t = Toile()
    t.fond_radial(28, 22, 42, [(120, 70, 170), (60, 24, 100), (24, 8, 40), (6, 2, 12)])
    fumee(t, 12, 34, 22, 18, graine=4)
    heros(t, "brasTendu", 20, 26, couleur=(20, 10, 30), liseres=(((210, 150, 255), 1, 0),))
    lame(t, 30, 16, 40, 12, garde=False, largeur=2, metal=(220, 200, 255))
    explosion(t, 40, 12, 6, couleurs=(BLANC, (230, 180, 255), (170, 80, 230)), branches=6)
    t.vignette(0.4)
    return t


def smokeScreen():
    """Gris violacé : une grande nappe de fumée sombre"""
    t = Toile()
    t.fond_radial(24, 30, 44, [(170, 160, 180), (100, 90, 120), (50, 40, 64), (16, 12, 22)])
    orbe(t, 24, 10, 3, (90, 80, 100), halo=False)
    fumee(t, 24, 34, 48, 20, graine=7, couleur=(70, 56, 90))
    fumee(t, 20, 26, 26, 12, graine=9, couleur=(90, 76, 110))
    t.vignette(0.4)
    return t


def phantomShot():
    """Violet fantôme : un carreau violet, le voleur réapparaît derrière la cible"""
    t = Toile()
    t.fond_radial(28, 24, 42, [(220, 180, 255), (140, 80, 210), (60, 24, 110), (16, 6, 30)])
    fleche(t, 4, 26, 22, 26, couleur=(220, 160, 255), trainee=(180, 100, 240), fut=(120, 60, 160))
    heros(t, "accroupi", 28, 30, s=0.8, couleur=(40, 20, 50), liseres=((BLANC, 0, -1),), retourne=True)
    m, _ = heros(t, "debout", 40, 26, s=0.8, couleur=(20, 10, 30), liseres=(((210, 150, 255), -1, 0),), retourne=True)
    fumee(t, 40, 38, 14, 6, graine=3)
    t.vignette(0.4)
    return t


def toxicCloud():
    """Vert opaque : le carreau libère un grand brouillard toxique"""
    t = Toile()
    t.fond_radial(24, 28, 44, [(140, 200, 80), (70, 120, 40), (24, 50, 16), (6, 16, 4)])
    nuage(t, 24, 30, 48, 26, (110, 190, 60), sombre=(50, 100, 30), graine=11, n=9)
    nuage(t, 24, 24, 26, 12, (160, 230, 90), graine=12)
    crane(t, 24, 30, 4, os=(220, 240, 180), yeux=(40, 70, 20))
    t.vignette(0.45)
    return t


def illusoryBarrage():
    """Pourpre sombre : le leurre crache une volée de carreaux d'ombre"""
    t = Toile()
    t.fond_radial(20, 24, 44, [(190, 120, 220), (110, 50, 150), (44, 16, 70), (12, 4, 20)])
    m, _ = heros(t, "tir", 12, 28, couleur=(70, 40, 100), liseres=(((210, 160, 255), 0, -1),))
    t.voile(m, (150, 100, 200), 0.3)
    for y in (12, 20, 28, 36):
        fleche(t, 24, y + 2, 42, y, couleur=(40, 20, 60), fut=(60, 30, 90), plumes=(160, 100, 220))
    t.vignette(0.4)
    return t


def voidEclipse():
    """Ultime : le ciel s'éteint, une ombre saute de cible en cible en laissant des traits cyan"""
    t = Toile()
    t.fond_radial(24, 14, 40, [(60, 30, 90), (24, 8, 46), (8, 2, 16), (2, 0, 4)])
    orbe(t, 24, 12, 7, (10, 4, 16), halo=False)
    anneau(t, 24, 12, 8, 8, CYAN, 1)
    pts = [(6, 40), (18, 30), (30, 40), (42, 28)]
    t.peindre(t.masque(lambda d: d.line(pts, fill=1, width=1)), CYAN)
    for x, y in pts[1:]:
        heros(t, "accroupi", x, y - 2, s=0.45, couleur=(14, 6, 22), liseres=((CYAN, 0, -1),))
    t.vignette(0.3)
    return t
