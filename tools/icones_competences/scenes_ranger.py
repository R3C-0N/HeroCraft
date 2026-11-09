"""Scènes du Rôdeur : deux arbalètes, des balles cyan et l'électricité dans l'air."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater

ELEC = (110, 240, 255)


def balle(t, x, y, couleur=ELEC, trainee=10):
    t.peindre(t.masque(lambda d: d.line([(x - trainee, y), (x, y)], fill=1, width=2)), melange(couleur, (0, 0, 0), 0.3))
    orbe(t, x, y, 2, couleur)


def twinShot():
    """Nuit sarcelle : deux balles cyan partent en même temps, une de chaque main"""
    t = Toile()
    t.fond_radial(30, 24, 44, [(120, 220, 230), (30, 120, 140), (10, 50, 70), (2, 18, 26)])
    arbalete(t, 4, 18, couleur=(60, 60, 70))
    arbalete(t, 4, 32, couleur=(60, 60, 70))
    balle(t, 36, 16, trainee=14)
    balle(t, 40, 30, trainee=14)
    t.vignette(0.4)
    return t


def evasiveRoll():
    """Ocre poussiéreux : une roulade dans la poussière"""
    t = Toile()
    t.fond_vertical([(150, 180, 190), (210, 190, 140), (190, 150, 90)], 0, 36)
    sol = t.masque(lambda d: d.rectangle([0, 38, 48, 48], fill=1))
    t.zone_verticale(sol, [(140, 100, 60), (70, 46, 26)], 38, 48)
    boule = t.masque(lambda d: d.ellipse([18, 22, 34, 38], fill=1))
    t.peindre(boule, (30, 30, 40))
    t.liseré(boule, ELEC, 0, -1)
    t.peindre(t.masque(lambda d: d.arc([14, 18, 38, 42], 160, 300, fill=1, width=2)), BLANC)
    nuage(t, 10, 38, 18, 8, (220, 200, 160), graine=4)
    t.vignette(0.4)
    return t


def tracerRound():
    """Bleu profond : une balle éclatante, la cible garde une marque lumineuse"""
    t = Toile()
    t.fond_radial(32, 26, 44, [(160, 220, 255), (50, 110, 190), (16, 40, 90), (4, 12, 30)])
    heros(t, "debout", 36, 30, s=0.8, couleur=(16, 20, 40), liseres=((ELEC, 0, -1),), retourne=True)
    mire(t, 36, 10, 4, couleur=ELEC)
    t.peindre(t.masque(lambda d: d.line([(2, 24), (26, 24)], fill=1, width=1)), ELEC)
    orbe(t, 26, 24, 3, BLANC)
    t.vignette(0.4)
    return t


def recoilShot():
    """Acier bleuté : un tir lourd projette un anneau de choc sur la cible"""
    t = Toile()
    t.fond_radial(32, 24, 44, [(200, 230, 240), (100, 140, 170), (40, 60, 90), (10, 18, 30)])
    for r in (6, 10, 14):
        anneau(t, 34, 24, r * 0.5, r, ELEC if r < 14 else BLANC, 2)
    arbalete(t, 2, 26, couleur=(60, 60, 70))
    t.peindre(t.masque(lambda d: d.line([(18, 26), (32, 24)], fill=1, width=3)), ELEC)
    t.vignette(0.4)
    return t


def fragRounds():
    """Cyan électrique : les arbalètes luisent, chaque tir éclate en bulle"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(180, 255, 255), (60, 190, 210), (20, 90, 120), (6, 30, 44)])
    t.lueur(16, 30, 10, ELEC, 0.7)
    arbalete(t, 4, 30, couleur=(60, 60, 70))
    for x, y, r in ((30, 14, 6), (40, 28, 4), (28, 36, 3)):
        bulle = t.masque(lambda d, x=x, y=y, r=r: d.ellipse([x - r, y - r, x + r, y + r], fill=1))
        t.voile(bulle, ELEC, 0.45)
        anneau(t, x, y, r, r, BLANC, 1)
    t.vignette(0.4)
    return t


def shockVolley():
    """Orage violet-bleu : les balles électriques rebondissent d'une cible à l'autre"""
    t = Toile()
    t.fond_radial(24, 20, 44, [(160, 150, 240), (70, 60, 170), (24, 20, 80), (6, 6, 24)])
    cibles = [(10, 34), (24, 18), (38, 32)]
    for (x0, y0), (x1, y1) in zip(cibles, cibles[1:]):
        eclair(t, zigzag(x0, y0, x1, y1, n=4, amp=3, graine=x0), couleur=ELEC)
    for x, y in cibles:
        heros(t, "debout", x, y + 4, s=0.45, couleur=(20, 20, 40), liseres=((ELEC, 0, -1),))
    t.vignette(0.4)
    return t


def coveringFire():
    """Rouge-orangé de bataille : un déluge de traits dans un cône"""
    t = Toile()
    t.fond_radial(10, 28, 46, [(255, 220, 160), (230, 120, 60), (140, 50, 30), (60, 16, 14), (20, 4, 4)])
    for a in (-30, -18, -6, 6, 18, 30):
        r = math.radians(a)
        x0, y0 = 12 + 8 * math.cos(r), 28 + 8 * math.sin(r)
        x1, y1 = 12 + 34 * math.cos(r), 28 + 34 * math.sin(r)
        t.peindre(t.masque(lambda d, p=(x0, y0, x1, y1): d.line([(p[0], p[1]), (p[2], p[3])], fill=1)), (255, 250, 200))
    arbalete(t, 2, 28, couleur=(60, 60, 70))
    t.vignette(0.4)
    return t


def deadlyFlip():
    """Ciel du soir : saut arrière, des traits pleuvent vers le sol"""
    t = Toile()
    t.fond_vertical([(30, 40, 90), (100, 110, 180), (230, 170, 140)], 0, 40)
    sol = t.masque(lambda d: d.rectangle([0, 40, 48, 48], fill=1))
    t.peindre(sol, (40, 30, 40))
    heros(t, "saut", 24, 14, s=0.8, liseres=((ELEC, 0, -1),), retourne=True)
    for x in (12, 20, 28, 36):
        t.peindre(t.masque(lambda d, x=x: d.line([(x + 2, 24), (x - 2, 38)], fill=1)), ELEC)
        t.etincelle(x - 2, 40, ELEC)
    t.vignette(0.4)
    return t


def detonatingMark():
    """Rouge et cyan : les tirs explosifs dévient vers la cible marquée"""
    t = Toile()
    t.fond_radial(30, 24, 44, [(255, 180, 160), (200, 70, 70), (100, 20, 40), (30, 6, 14)])
    heros(t, "debout", 32, 30, s=0.8, couleur=(30, 10, 20), liseres=((BLANC, 0, -1),), retourne=True)
    mire(t, 32, 20, 9, couleur=(255, 80, 60))
    for y0 in (8, 36):
        t.peindre(t.masque(lambda d, y0=y0: d.arc([2, min(y0, 20), 34, max(y0, 20) + 8], 180 if y0 < 20 else 90, 270 if y0 < 20 else 180,
                                                  fill=1, width=1)), ELEC)
    t.vignette(0.4)
    return t


def magneticStorm():
    """Violet crépitant : le cône devient un champ d'éclairs, les cibles sont étourdies"""
    t = Toile()
    t.fond_radial(28, 24, 44, [(220, 200, 255), (130, 90, 220), (50, 30, 120), (14, 8, 40)])
    for i, (y0, y1) in enumerate(((10, 16), (24, 26), (38, 32))):
        eclair(t, zigzag(4, 24, 44, y0 if i != 1 else y1, n=6, amp=3, graine=i + 3), couleur=ELEC)
    for x, y in ((40, 12), (44, 34)):
        t.etincelle(x, y, (255, 240, 80), grande=True)
    t.vignette(0.4)
    return t


def bulletDance():
    """Turquoise : le rôdeur tournoie et arrose tout autour de lui"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(170, 255, 230), (50, 180, 170), (14, 80, 90), (4, 26, 34)])
    anneau(t, 24, 26, 13, 13, BLANC, 1)
    heros(t, "brasOuverts", 24, 28, s=0.8, liseres=((ELEC, 0, -1),))
    for i in range(8):
        a = math.radians(i * 45)
        balle(t, 24 + 19 * math.cos(a), 26 + 19 * math.sin(a), trainee=0)
    t.vignette(0.4)
    return t


def voidDeluge():
    """Ultime : le rôdeur lévite, une onde libère des centaines de traits cyan"""
    t = Toile()
    t.fond_radial(24, 22, 42, [(90, 40, 160), (46, 14, 90), (16, 4, 40), (4, 0, 12)])
    for i in range(16):
        a = math.radians(i * 22.5 + 5)
        t.peindre(t.masque(lambda d, a=a: d.line([(24 + 12 * math.cos(a), 22 + 12 * math.sin(a)),
                                                   (24 + 26 * math.cos(a), 22 + 26 * math.sin(a))], fill=1)), CYAN)
    anneau(t, 24, 22, 11, 11, (200, 150, 255), 2)
    heros(t, "brasOuverts", 24, 24, s=0.6, liseres=((CYAN, 0, -1),))
    t.vignette(0.3)
    return t
