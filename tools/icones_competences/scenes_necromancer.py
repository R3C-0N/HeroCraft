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
    portail(t, 24, 40, 16, 4, (150, 255, 150), (20, 40, 20), lueur=VERT)
    bete(t, "squelette", 24, 28, s=0.95, couleur=OS, liseres=((BLANC, 0, -1), ((120, 110, 90), 0, 1)))
    crane(t, 24, 13, 3.5, yeux=(80, 255, 120))
    for x, y in ((8, 34), (40, 32), (12, 28), (37, 26)):
        t.point(x, y, (110, 80, 50)); t.point(x + 1, y - 1, (80, 56, 36))
    t.vignette(0.45)
    return t


def soulSiphon():
    """Sarcelle sombre : un filet d'âme quitte la cible et vient se perdre dans la main"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(60, 140, 130), (24, 80, 80), (10, 36, 40), (4, 14, 16)])
    heros(t, "tombe", 36, 36, s=0.8, couleur=(30, 30, 40), liseres=((VERT, 0, -1),), retourne=True)
    pts = [(36, 30), (30, 22), (22, 26), (14, 20)]
    t.peindre(t.masque(lambda d: d.line(pts, fill=1, width=3, joint="curve")), (60, 200, 100))
    t.peindre(t.masque(lambda d: d.line(pts, fill=1, width=1, joint="curve")), VERT_PALE)
    for x, y in ((33, 26), (26, 23), (18, 23)):
        t.etincelle(x, y, VERT)
    main = t.masque(lambda d: (d.ellipse([4, 16, 14, 26], fill=1), [d.line([(6 + i * 2.5, 18), (8 + i * 3, 11)], fill=1, width=2) for i in range(4)]))
    t.objet(main, (190, 200, 180), (230, 240, 220), (100, 110, 100), lumiere=(1, -1))
    t.lueur(10, 20, 7, VERT, 0.5)
    t.vignette(0.45)
    return t


def boneWall():
    """Violet de crépuscule : des os pointus jaillissent du sol en barricade"""
    t = Toile()
    t.fond_vertical([(40, 20, 60), (110, 60, 120), (170, 120, 150)], 0, 34)
    sol = t.masque(lambda d: d.rectangle([0, 34, 48, 48], fill=1))
    t.zone_verticale(sol, [(70, 50, 60), (30, 20, 30)], 34, 48)
    pics(t, 2, 46, 38, 26, couleur=OS, n=6)
    pics(t, 6, 42, 40, 14, couleur=(200, 190, 160), n=5)
    t.lueur(24, 36, 14, VERT, 0.25, sy=0.5)
    t.vignette(0.4)
    return t


def poltergeist():
    """Bleu de minuit : un crâne spectral hurlant sort de la lanterne en laissant une traînée"""
    t = Toile()
    t.fond_radial(28, 18, 44, [(80, 110, 160), (36, 50, 100), (14, 20, 50), (4, 6, 20)])
    trainee = t.masque(lambda d: d.polygon([(4, 44), (10, 36), (26, 20), (30, 26), (12, 42)], fill=1))
    t.voile(trainee, VERT, 0.45)
    lanterne = t.masque(lambda d: d.rectangle([4, 38, 11, 46], fill=1))
    t.objet(lanterne, (90, 80, 60), (150, 140, 110), (40, 30, 20))
    t.lueur(8, 42, 5, VERT, 0.8)
    t.lueur(32, 16, 12, VERT, 0.6)
    crane(t, 32, 15, 8, os=(200, 255, 210), yeux=(20, 60, 30))
    t.peindre(t.masque(lambda d: d.ellipse([29, 20, 35, 26], fill=1)), (20, 50, 30))
    t.vignette(0.45)
    return t


def deathKnight():
    """Gris de cendre : un large portail fumant, le chevalier noir en sort"""
    t = Toile()
    t.fond_radial(24, 30, 42, [(150, 150, 140), (90, 90, 90), (40, 40, 46), (12, 12, 16)])
    portail(t, 24, 42, 19, 4, (120, 255, 140), (10, 20, 12), lueur=VERT)
    nuage(t, 24, 38, 36, 10, (40, 40, 50), graine=7)
    m, q = heros(t, "garde", 24, 25, s=1.1, couleur=(24, 26, 34), liseres=((VERT, 0, -1),))
    t.point(25, 10, (255, 50, 40)); t.point(27, 10, (255, 50, 40))
    lame(t, 32, 26, 40, 6, metal=(140, 150, 170), lueur=VERT)
    t.vignette(0.45)
    return t


def miasmaCloud():
    """Marais olive : un nuage vert sale stagne au sol et bouillonne"""
    t = Toile()
    t.fond_vertical([(30, 40, 20), (80, 100, 40), (120, 130, 60)], 0, 36)
    sol = t.masque(lambda d: d.rectangle([0, 36, 48, 48], fill=1))
    t.zone_verticale(sol, [(50, 60, 30), (20, 26, 12)], 36, 48)
    nuage(t, 24, 32, 46, 16, (120, 170, 60), sombre=(60, 90, 30), graine=3, n=8)
    nuage(t, 22, 28, 28, 10, (160, 210, 80), graine=5, n=5)
    for x, y in ((12, 22), (30, 20), (36, 26), (18, 18)):
        anneau(t, x, y, 1.5, 1.5, (200, 240, 120), 1)
    crane(t, 24, 40, 3, os=(200, 200, 150), yeux=(40, 60, 20))
    t.vignette(0.45)
    return t


def fleshGolem():
    """Rouge viande : le golem s'amalgame dans une gerbe de chair et de sang"""
    t = Toile()
    t.fond_radial(24, 26, 42, [(240, 140, 130), (170, 50, 60), (90, 16, 30), (30, 4, 10)])
    bete(t, "golem", 24, 26, s=1.2, couleur=(150, 80, 90), liseres=(((230, 150, 150), 0, -1), ((70, 20, 30), 0, 1)))
    for y in (16, 24, 30):
        t.peindre(t.masque(lambda d, y=y: d.line([(18, y), (30, y + 2)], fill=1)), (90, 30, 40))
    t.point(22, 11, VERT); t.point(26, 11, VERT)
    sang(t, ((8, 12, 2), (40, 14, 2.5), (6, 30, 1.5), (42, 34, 2), (12, 40, 1.5)))
    t.vignette(0.4)
    return t


def spectralGrasp():
    """Noir et vert : un sceau s'allume au sol, des mains fantomatiques en jaillissent"""
    t = Toile()
    t.fond_radial(24, 36, 42, [(60, 140, 80), (20, 70, 40), (6, 30, 16), (2, 10, 6)])
    anneau(t, 24, 38, 20, 6, VERT, 2, clair=BLANC)
    anneau(t, 24, 38, 13, 4, (90, 200, 110), 1)
    t.lueur(24, 34, 16, VERT, 0.5, sy=1.4)
    for x, h in ((14, 16), (24, 24), (34, 18)):
        bras = t.masque(lambda d, x=x, h=h: (d.line([(x, 38), (x + 1, 38 - h)], fill=1, width=3),
                                             [d.line([(x + 1, 38 - h), (x - 2 + i * 2, 33 - h)], fill=1, width=1) for i in range(4)]))
        t.voile(bras, (200, 255, 210), 0.7)
        t.liseré(bras, BLANC, -1, 0)
    t.vignette(0.45)
    return t


def lichLord():
    """Violet profond : la liche couronnée lève son bâton, des orbes verts flottent"""
    t = Toile()
    t.fond_radial(24, 22, 42, [(150, 90, 200), (80, 30, 130), (36, 10, 60), (10, 2, 20)])
    nuage(t, 24, 42, 46, 10, (30, 16, 40), graine=2)
    m, q = heros(t, "brasTendu", 22, 28, s=1.05, couleur=(30, 20, 40), liseres=((VERT, 0, -1),), cape=True)
    for x in (18, 21, 24):
        t.peindre(t.masque(lambda d, x=x: d.polygon([(x, 11), (x + 1.5, 7), (x + 3, 11)], fill=1)), OR)
    t.point(21, 14, VERT); t.point(24, 14, VERT)
    baton(t, 36, 40, 36, 12, bois=(80, 60, 70), gemme=VERT)
    for x, y in ((8, 14), (10, 30), (42, 26)):
        orbe(t, x, y, 2.5, VERT)
    t.vignette(0.4)
    return t


def boneBehemoth():
    """Sépia d'ossuaire : un béhémoth d'os charge dans la poussière"""
    t = Toile()
    t.fond_vertical([(70, 50, 40), (160, 120, 80), (210, 180, 130)], 0, 38)
    sol = t.masque(lambda d: d.rectangle([0, 38, 48, 48], fill=1))
    t.zone_verticale(sol, [(110, 80, 60), (50, 34, 24)], 38, 48)
    nuage(t, 8, 38, 16, 8, (190, 160, 120), graine=4)
    bete(t, "ours", 26, 26, s=1.15, couleur=(220, 210, 180), liseres=((BLANC, 0, -1), ((120, 110, 90), 0, 1)))
    for x in (18, 22, 26, 30):
        t.peindre(t.masque(lambda d, x=x: d.line([(x, 22), (x - 1, 30)], fill=1)), (120, 100, 80))
    pics(t, 18, 34, 18, 6, couleur=OS, n=4)
    t.point(40, 22, VERT)
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
