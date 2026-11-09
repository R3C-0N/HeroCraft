"""Scènes du Nécromancien : lanterne, bâton, et tout ce qui remonte de sous la terre."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater

VERT = (150, 255, 150)
VERT_PALE = (200, 255, 200)
OS = (230, 225, 200)


def skeletonServant():
    """Vert malade : un portail au sol, le squelette en sort dans une gerbe de terre"""
    t = Toile()
    t.fond_radial(24, 38, 44, [(120, 200, 110), (50, 110, 60), (20, 50, 30), (6, 18, 10)])
    portail(t, 24, 41, 17, 4, (150, 255, 150), (20, 40, 20), lueur=VERT)
    corps, q = heros(t, "brasOuverts", 24, 27, s=1.05, couleur=OS, liseres=((BLANC, 0, -1), ((120, 110, 90), 0, 1)))
    t.contour(corps, (20, 30, 20))
    for y in (17, 19, 21, 23):
        t.peindre(t.masque(lambda d, y=y: d.line([(21, y), (27, y)], fill=1)), (60, 70, 50))
    t.peindre(t.masque(lambda d: d.line([(24, 16), (24, 27)], fill=1)), (60, 70, 50))
    crane(t, 24, 11, 3.6, yeux=(80, 255, 120))
    for x, y in ((8, 36), (40, 35), (11, 30), (37, 29)):
        t.peindre(t.masque(lambda d, x=x, y=y: d.ellipse([x - 1.5, y - 1.5, x + 1.5, y + 1.5], fill=1)), (110, 80, 50))
    t.vignette(0.45)
    return t


def soulSiphon():
    """Sarcelle sombre : un filet d'âme quitte l'ennemi et vient se perdre dans la main du nécromancien"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(60, 140, 130), (24, 80, 80), (10, 36, 40), (4, 14, 16)])
    heros(t, "brasTendu", 10, 30, s=0.9, liseres=((VERT, 1, 0),))
    heros(t, "debout", 38, 30, s=0.85, couleur=(40, 50, 50), liseres=((VERT_PALE, -1, 0),), retourne=True)
    pts = [(36, 20), (30, 14), (24, 20), (19, 18)]
    t.peindre(t.masque(lambda d: d.line(pts, fill=1, width=3, joint="curve")), (60, 200, 100))
    t.peindre(t.masque(lambda d: d.line(pts, fill=1, width=1, joint="curve")), VERT_PALE)
    for x, y in ((33, 16), (27, 16)):
        t.etincelle(x, y, VERT)
    t.lueur(19, 18, 6, VERT, 0.6)
    t.vignette(0.45)
    return t


def boneWall():
    """Violet de crépuscule : de grands os jaillissent du sol en barricade"""
    t = Toile()
    t.fond_vertical([(40, 20, 60), (110, 60, 120), (170, 120, 150)], 0, 36)
    sol = t.masque(lambda d: d.rectangle([0, 38, 48, 48], fill=1))
    t.zone_verticale(sol, [(70, 50, 60), (30, 20, 30)], 38, 48)
    for x, h, a in ((6, 18, -10), (13, 26, 5), (20, 20, -6), (27, 30, 4), (34, 22, -5), (41, 16, 8)):
        dx = h * math.sin(math.radians(a))
        os_(t, x, 40, x + dx, 40 - h, couleur=OS)
    t.vignette(0.4)
    return t


def poltergeist():
    """Bleu de minuit : un crâne spectral hurlant sort de la lanterne en laissant une traînée"""
    t = Toile()
    t.fond_radial(28, 18, 44, [(80, 110, 160), (36, 50, 100), (14, 20, 50), (4, 6, 20)])
    trainee = t.masque(lambda d: d.polygon([(10, 36), (26, 20), (32, 26), (14, 40)], fill=1))
    t.voile(trainee, VERT, 0.45)
    lanterne(t, 9, 39)
    t.lueur(32, 16, 12, VERT, 0.6)
    crane(t, 32, 15, 8, os=(200, 255, 210), yeux=(20, 60, 30))
    t.peindre(t.masque(lambda d: d.ellipse([29, 20, 35, 26], fill=1)), (20, 50, 30))
    t.vignette(0.45)
    return t


def deathKnight():
    """Gris de cendre : un large portail fumant, le chevalier noir en sort, épée au clair"""
    t = Toile()
    t.fond_radial(24, 26, 42, [(170, 170, 160), (100, 100, 100), (44, 44, 50), (12, 12, 16)])
    portail(t, 24, 42, 20, 4, (120, 255, 140), (10, 20, 12), lueur=VERT)
    heros(t, "garde", 22, 28, s=1.1, couleur=(24, 26, 34), liseres=((VERT, 0, -1), ((150, 160, 170), -1, 0)))
    t.point(23, 12, (255, 50, 40)); t.point(25, 12, (255, 50, 40))
    lame(t, 31, 23, 40, 6, metal=(150, 160, 180), lueur=VERT)
    t.vignette(0.45)
    return t


def miasmaCloud():
    """Marais olive : un nuage vert sale stagne au sol et bouillonne"""
    t = Toile()
    t.fond_vertical([(30, 40, 20), (80, 100, 40), (120, 130, 60)], 0, 36)
    sol = t.masque(lambda d: d.rectangle([0, 36, 48, 48], fill=1))
    t.zone_verticale(sol, [(50, 60, 30), (20, 26, 12)], 36, 48)
    nuage(t, 24, 34, 50, 18, (120, 170, 50), graine=3, n=9, alpha=0.55)
    nuage(t, 22, 30, 34, 12, (160, 210, 70), graine=5, n=6, alpha=0.5)
    for x, y, r in ((12, 24, 2), (30, 20, 2.5), (36, 28, 1.5), (18, 16, 1.5), (26, 30, 1.5)):
        anneau(t, x, y, r, r, (210, 250, 130), 1)
    t.vignette(0.45)
    return t


def fleshGolem():
    """Rouge viande : le golem s'amalgame dans une gerbe de chair et de sang"""
    t = Toile()
    t.fond_radial(24, 22, 42, [(255, 190, 170), (200, 80, 80), (110, 24, 40), (36, 4, 12)])
    bete(t, "golem", 24, 25, s=1.1, couleur=(190, 110, 110), liseres=(((250, 190, 180), 0, -1), ((100, 40, 50), 0, 1)))
    for y in (16, 22, 28):
        t.peindre(t.masque(lambda d, y=y: d.line([(18, y), (30, y + 2)], fill=1)), (110, 40, 50))
    t.point(22, 9, VERT); t.point(26, 9, VERT)
    sang(t, ((7, 12, 2), (41, 14, 2.5), (6, 30, 1.5), (42, 34, 2)))
    t.vignette(0.35)
    return t


def spectralGrasp():
    """Noir et vert : un sceau s'allume au sol, des mains fantomatiques en jaillissent"""
    t = Toile()
    t.fond_radial(24, 36, 42, [(60, 140, 80), (20, 70, 40), (6, 30, 16), (2, 10, 6)])
    anneau(t, 24, 40, 21, 5, VERT, 2, clair=BLANC)
    t.lueur(24, 34, 16, VERT, 0.5, sy=1.4)
    for x, y, a in ((12, 28, -20), (24, 20, 0), (36, 28, 20)):
        main_ouverte(t, x, y, s=1.3, couleur=(200, 255, 210), alpha=0.75, angle=a)
    t.vignette(0.45)
    return t


def lichLord():
    """Violet profond : la liche couronnée lève son bâton, des orbes verts flottent"""
    t = Toile()
    t.fond_radial(24, 22, 42, [(150, 90, 200), (80, 30, 130), (36, 10, 60), (10, 2, 20)])
    heros(t, "brasTendu", 20, 28, s=1.05, couleur=(30, 20, 40), liseres=((VERT, 0, -1), ((150, 100, 200), -1, 0)))
    for x in (16, 19, 22):
        t.peindre(t.masque(lambda d, x=x: d.polygon([(x, 13), (x + 1.5, 9), (x + 3, 13)], fill=1)), OR)
    t.point(19, 15, VERT); t.point(22, 15, VERT)
    baton(t, 33, 42, 33, 12, bois=(80, 60, 70), gemme=VERT)
    for x, y in ((8, 12), (8, 30), (42, 24)):
        orbe(t, x, y, 2.5, VERT)
    t.vignette(0.4)
    return t


def boneBehemoth():
    """Sépia d'ossuaire : un béhémoth d'os charge dans la poussière"""
    t = Toile()
    t.fond_vertical([(70, 50, 40), (160, 120, 80), (210, 180, 130)], 0, 38)
    sol = t.masque(lambda d: d.rectangle([0, 38, 48, 48], fill=1))
    t.zone_verticale(sol, [(110, 80, 60), (50, 34, 24)], 38, 48)
    nuage(t, 6, 38, 14, 8, (190, 160, 120), graine=4)
    bete(t, "ours", 22, 28, s=1.05, couleur=(225, 215, 185), liseres=((BLANC, 0, -1), ((120, 110, 90), 0, 1)))
    for x in (14, 18, 22, 26):
        t.peindre(t.masque(lambda d, x=x: d.arc([x - 3, 22, x + 3, 34], 200, 340, fill=1)), (110, 90, 70))
    crane(t, 37, 25, 5, os=OS, yeux=VERT)
    pics(t, 12, 30, 22, 5, couleur=OS, n=4)
    t.vignette(0.4)
    return t


def soulStorm():
    """Nuit verte : un tourbillon d'âmes happe tout ce qui l'entoure"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(70, 160, 110), (24, 80, 60), (8, 34, 26), (2, 12, 10)])
    for i, r in enumerate((19, 14, 9)):
        t.peindre(t.masque(lambda d, r=r, i=i: d.arc([24 - r, 24 - r, 24 + r, 24 + r], i * 60, i * 60 + 250, fill=1, width=2)),
                  (120, 230, 150) if i != 1 else VERT_PALE)
    t.lueur(24, 24, 8, VERT_PALE, 0.8)
    for a in (30, 150, 270):
        x, y = 24 + 17 * math.cos(math.radians(a)), 24 + 17 * math.sin(math.radians(a))
        crane(t, x, y, 2.5, os=(200, 255, 210), yeux=(20, 60, 30))
    t.vignette(0.4)
    return t


def voidLegion():
    """Ultime : le bâton fend le sol d'Étherium, les ombres implosent sous un bouclier violet"""
    t = Toile()
    t.fond_radial(24, 30, 42, [(90, 40, 150), (46, 14, 90), (18, 4, 40), (6, 2, 14)])
    sol = t.masque(lambda d: d.rectangle([0, 38, 48, 48], fill=1))
    t.zone_verticale(sol, [(30, 16, 40), (10, 4, 14)], 38, 48)
    fissure(t, 4, 42, 44, 40)
    t.lueur(24, 40, 14, CYAN, 0.4, sy=0.4)
    for x in (9, 39):
        heros(t, "debout", x, 30, s=0.6, couleur=(20, 6, 30), liseres=((VIOLET, 0, -1),))
    dome = t.masque(lambda d: d.pieslice([12, 14, 36, 62], 180, 360, fill=1))
    t.voile(dome, VIOLET, 0.35)
    t.peindre(t.masque(lambda d: d.arc([12, 14, 36, 62], 180, 360, fill=1, width=1)), (200, 140, 255))
    heros(t, "brasTendu", 22, 30, s=0.8, liseres=((CYAN, 0, -1),))
    baton(t, 30, 42, 31, 18, bois=(70, 50, 70), gemme=CYAN)
    t.vignette(0.35)
    return t
