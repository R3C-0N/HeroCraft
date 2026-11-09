"""Scènes du Voleur : arbalète de poing, ombre et poison, toujours un peu dans la nuit."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater

OMBRE_V = (60, 30, 80)
POISON = (130, 230, 60)


def fumee(t, cx, cy, w, h, graine=1, couleur=(70, 40, 100), alpha=0.6):
    """Fumée d'ombre translucide, en volutes superposées"""
    import random
    rnd = random.Random(graine)
    for i in range(6):
        x, y = cx + rnd.uniform(-w / 2, w / 2) * 0.8, cy + rnd.uniform(-h / 2, h / 2) * 0.6
        r = rnd.uniform(h * 0.3, h * 0.55)
        m = t.masque(lambda d, x=x, y=y, r=r: d.ellipse([x - r * 1.3, y - r, x + r * 1.3, y + r], fill=1))
        t.voile(m, couleur, alpha * (0.6 + 0.4 * (i % 2)))
        t.peindre(t.masque(lambda d, x=x, y=y, r=r: d.arc([x - r * 1.3, y - r, x + r * 1.3, y + r], 200, 300, fill=1)),
                  melange(couleur, BLANC, 0.35))


def quickShot():
    """Nuit bleu acier : un carreau très rapide avec une traînée courte"""
    t = Toile()
    t.fond_radial(30, 22, 44, [(170, 190, 230), (80, 100, 160), (30, 40, 80), (8, 12, 30)])
    xa = arbalete(t, 4, 30, couleur=(80, 56, 60), carreau=False)
    for y in (22, 34):
        t.peindre(t.masque(lambda d, y=y: d.line([(24, y), (34, y)], fill=1)), (200, 210, 240))
    fleche(t, 26, 28, 44, 28, trainee=(220, 230, 255), plumes=(160, 60, 200), epais=2)
    t.vignette(0.4)
    return t


def poisonBolt():
    """Vert de venin : le voleur tire un carreau à traînée verte, la cible fume de vapeur toxique"""
    t = Toile()
    t.fond_radial(30, 28, 44, [(180, 230, 120), (80, 150, 50), (30, 70, 24), (8, 24, 8)])
    heros(t, "tir", 9, 30, s=0.8, liseres=((POISON, 1, 0),))
    heros(t, "accroupi", 38, 31, s=0.85, couleur=(30, 40, 20), liseres=((POISON, 0, -1),), retourne=True)
    fumee(t, 38, 16, 12, 8, graine=4, couleur=POISON, alpha=0.5)
    fleche(t, 18, 22, 32, 22, couleur=POISON, trainee=(160, 255, 90), fut=(80, 60, 50))
    t.vignette(0.45)
    return t


def shadowVeil():
    """Violet d'encre : une bouffée de fumée d'ombre, le voleur disparaît"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(150, 100, 190), (80, 40, 120), (34, 14, 56), (10, 4, 18)])
    corps, _ = heros(t, "debout", 24, 26, couleur=(40, 20, 60), liseres=(((180, 120, 230), 0, -1),))
    fumee(t, 24, 36, 40, 14, graine=3)
    fumee(t, 24, 20, 30, 12, graine=5, alpha=0.4)
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
    arbalete(t, 3, 30, couleur=(80, 56, 60), carreau=False)
    fleche(t, 20, 28, 26, 24, epais=2)
    explosion(t, 30, 22, 6, couleurs=(BLANC, (220, 230, 255), (150, 160, 190)), branches=7)
    for a in (-60, -30, 0, 30, 60):
        r = math.radians(a)
        t.peindre(t.masque(lambda d, r=r: d.line([(30 + 8 * math.cos(r), 22 + 8 * math.sin(r)), (30 + 14 * math.cos(r), 22 + 14 * math.sin(r))],
                                                  fill=1)), (230, 235, 250))
    t.vignette(0.4)
    return t


def grappleBolt():
    """Toits au crépuscule : un câble tendu, le voleur s'y fait tirer"""
    t = Toile()
    t.fond_vertical([(40, 30, 70), (150, 80, 110), (240, 150, 100)], 0, 48)
    toit = t.masque(lambda d: d.polygon([(32, 16), (48, 8), (48, 48), (32, 48)], fill=1))
    t.peindre(toit, (24, 16, 30))
    t.liseré(toit, (255, 180, 140), 0, -1)
    corps, q = heros(t, "brasTendu", 12, 30, s=0.85, couleur=(30, 16, 40), liseres=(((255, 180, 140), 1, 0),))
    mx, my = q["mainD"]
    t.peindre(t.masque(lambda d: d.line([(mx, my), (33, 15)], fill=1)), (230, 220, 200))
    t.peindre(t.masque(lambda d: d.line([(33, 15), (36, 13)], fill=1, width=2)), (200, 200, 210))
    t.vignette(0.4)
    return t


def ambush():
    """Noir et éclat violet : le voleur surgit de la fumée et frappe de sa dague"""
    t = Toile()
    t.fond_radial(28, 22, 42, [(120, 70, 170), (60, 24, 100), (24, 8, 40), (6, 2, 12)])
    fumee(t, 10, 30, 18, 20, graine=4)
    corps, q = heros(t, "brasTendu", 22, 27, couleur=(20, 10, 30), liseres=(((210, 150, 255), 1, 0),))
    mx, my = q["mainD"]
    lame(t, mx - 1, my + 1, mx + 9, my - 3, garde=True, largeur=2, metal=(220, 200, 255))
    explosion(t, mx + 10, my - 4, 5, couleurs=(BLANC, (230, 180, 255), (170, 80, 230)), branches=6)
    t.vignette(0.4)
    return t


def smokeScreen():
    """Gris violacé : une grande nappe de fumée sombre se déploie"""
    t = Toile()
    t.fond_radial(24, 30, 44, [(170, 160, 180), (100, 90, 120), (50, 40, 64), (16, 12, 22)])
    fumee(t, 24, 32, 46, 20, graine=7, couleur=(60, 46, 80), alpha=0.7)
    fumee(t, 24, 22, 30, 12, graine=9, couleur=(90, 76, 110), alpha=0.5)
    bombe = t.masque(lambda d: d.ellipse([20, 6, 28, 14], fill=1))
    t.objet(bombe, (60, 50, 70), (120, 110, 130), (20, 16, 24))
    t.peindre(t.masque(lambda d: d.line([(26, 6), (29, 3)], fill=1)), (200, 180, 140))
    t.etincelle(30, 2, (255, 200, 80))
    t.vignette(0.4)
    return t


def phantomShot():
    """Violet fantôme : un carreau violet, le voleur réapparaît derrière la cible"""
    t = Toile()
    t.fond_radial(28, 24, 42, [(220, 180, 255), (140, 80, 210), (60, 24, 110), (16, 6, 30)])
    fleche(t, 2, 24, 16, 24, couleur=(220, 160, 255), trainee=(180, 100, 240), fut=(120, 60, 160))
    heros(t, "debout", 23, 28, s=0.85, couleur=(40, 20, 50), liseres=((BLANC, 0, -1),), retourne=True)
    fumee(t, 41, 36, 12, 8, graine=3, alpha=0.5)
    heros(t, "brasTendu", 40, 28, s=0.85, couleur=(20, 10, 30), liseres=(((210, 150, 255), -1, 0),), retourne=True)
    t.vignette(0.4)
    return t


def toxicCloud():
    """Vert opaque : le carreau éclate et libère un grand brouillard toxique"""
    t = Toile()
    t.fond_radial(24, 28, 44, [(140, 200, 80), (70, 120, 40), (24, 50, 16), (6, 16, 4)])
    fumee(t, 26, 30, 44, 22, graine=11, couleur=(140, 220, 70), alpha=0.6)
    fumee(t, 26, 24, 26, 12, graine=12, couleur=(190, 250, 110), alpha=0.5)
    fleche(t, 2, 14, 18, 22, couleur=(170, 255, 90), fut=(80, 60, 50))
    for x, y in ((18, 32), (32, 20), (36, 34)):
        anneau(t, x, y, 1.5, 1.5, (220, 255, 150), 1)
    t.vignette(0.45)
    return t


def illusoryBarrage():
    """Pourpre sombre : le leurre d'ombre crache une volée de carreaux"""
    t = Toile()
    t.fond_radial(20, 24, 44, [(190, 120, 220), (110, 50, 150), (44, 16, 70), (12, 4, 20)])
    m, _ = heros(t, "tir", 11, 28, s=0.95, couleur=(150, 100, 200), liseres=((BLANC, 0, -1), ((80, 40, 120), 0, 1)))
    t.contour(m, (40, 10, 60))
    for y in (12, 20, 28, 36):
        fleche(t, 26, y + 2, 44, y, couleur=(30, 10, 40), fut=(50, 20, 70), plumes=(220, 170, 255))
    t.vignette(0.4)
    return t


def voidEclipse():
    """Ultime : le ciel s'éteint, une ombre saute de cible en cible en laissant des traits cyan"""
    t = Toile()
    t.fond_radial(24, 14, 40, [(60, 30, 90), (24, 8, 46), (8, 2, 16), (2, 0, 4)])
    orbe(t, 24, 10, 6, (10, 4, 16), halo=False)
    anneau(t, 24, 10, 7, 7, CYAN, 1)
    t.peindre(t.masque(lambda d: d.line([(4, 44), (14, 26), (26, 38), (36, 24)], fill=1, width=1)), CYAN)
    for x in (12, 36):
        heros(t, "debout", x, 33, s=0.65, couleur=(70, 50, 90), liseres=(((200, 180, 220), 0, -1),))
    heros(t, "saut", 26, 30, s=0.85, couleur=(8, 2, 14), liseres=((CYAN, 0, -1), (CYAN, 1, 0)))
    t.vignette(0.3)
    return t

