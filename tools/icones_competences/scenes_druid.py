"""Scènes du Druide : métamorphoses, forêt et bêtes, chaque forme sa saison."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater, silhouette

FEUILLE = (120, 200, 70)
FEUILLE_S = (60, 130, 40)


def feuilles(t, pts, couleur=FEUILLE):
    for x, y, a in pts:
        plume(t, x, y, a, couleur=couleur, long=5)


def packClaws():
    """Vert de sous-bois : le bras devenu patte lacère l'air de trois griffures"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(170, 230, 120), (70, 150, 60), (24, 80, 30), (8, 30, 12)])
    for i in range(3):
        o = (i - 1) * 8
        entaille = t.masque(lambda d, o=o: d.polygon([(14 + o, 6), (18 + o, 8), (34 + o, 40), (31 + o, 42)], fill=1))
        t.peindre(entaille, (120, 10, 20))
        t.liseré(entaille, (255, 240, 230), -1, 0)
    patte = t.masque(lambda d: (d.ellipse([2, 2, 14, 12], fill=1),
                                [d.polygon([(4 + i * 3.5, 10), (6 + i * 3.5, 10), (7 + i * 3.5, 17)], fill=1) for i in range(3)]))
    t.objet(patte, (90, 60, 40), (150, 110, 70), (40, 24, 14))
    feuilles(t, ((40, 8, 120), (40, 38, 200), (6, 34, -40)))
    t.vignette(0.4)
    return t


def bearHide():
    """Ambre chaud : une peau d'ours retombe sur le dos du druide, un halo brun l'enveloppe"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(250, 210, 140), (210, 140, 60), (130, 70, 30), (50, 24, 10)])
    t.lueur(24, 24, 18, (200, 130, 60), 0.6)
    heros(t, "garde", 22, 28, s=1.05, liseres=((OR_PALE, 0, -1),))
    peau = t.masque(lambda d: (d.polygon([(13, 16), (19, 14), (21, 30), (12, 32)], fill=1),
                               d.ellipse([17, 6, 27, 15], fill=1), d.ellipse([17, 4, 21, 8], fill=1), d.ellipse([23, 4, 27, 8], fill=1)))
    t.objet(peau, (120, 76, 40), (180, 130, 80), (60, 34, 18))
    museau = t.masque(lambda d: d.ellipse([24, 10, 29, 14], fill=1))
    t.objet(museau, (160, 120, 80), (200, 160, 110), (90, 60, 30))
    t.point(23, 9, (20, 10, 10))
    for x, y in ((11, 20), (11, 26), (11, 31)):
        t.peindre(t.masque(lambda d, x=x, y=y: d.line([(x, y), (x - 2, y + 2)], fill=1)), (90, 56, 30))
    t.vignette(0.4)
    return t


def elkLegs():
    """Fauve des plaines : une patte d'élan, jarret et sabot fendu, soulève la poussière"""
    t = Toile()
    t.fond_vertical([(200, 170, 110), (230, 200, 140), (180, 140, 90)], 0, 38)
    sol = t.masque(lambda d: d.rectangle([0, 38, 48, 48], fill=1))
    t.zone_verticale(sol, [(140, 100, 60), (70, 46, 26)], 38, 48)
    cuisse = t.masque(lambda d: (d.polygon([(16, 0), (32, 0), (30, 14), (22, 18)], fill=1),
                                 d.line([(26, 14), (32, 22)], fill=1, width=4),
                                 d.line([(32, 22), (26, 36)], fill=1, width=3)))
    t.objet(cuisse, (150, 100, 60), (200, 150, 100), (80, 50, 30))
    sabot = t.masque(lambda d: d.polygon([(22, 35), (29, 35), (30, 40), (21, 40)], fill=1))
    t.objet(sabot, (40, 30, 26), (90, 70, 60), (20, 14, 10))
    t.peindre(t.masque(lambda d: d.line([(25, 37), (25, 40)], fill=1)), (150, 110, 70))
    nuage(t, 12, 40, 18, 7, (220, 190, 140), graine=6)
    nuage(t, 38, 41, 14, 6, (210, 180, 130), graine=8)
    t.vignette(0.4)
    return t


def galeFeathers():
    """Bleu-vert du large : des bras couverts de plumes battent et projettent une rafale"""
    t = Toile()
    t.fond_radial(18, 24, 44, [(220, 255, 250), (110, 210, 210), (40, 130, 150), (14, 60, 80), (4, 24, 34)])
    corps, q = heros(t, "brasOuverts", 14, 28, s=0.95, liseres=((BLANC, 1, 0),))
    for nom in ("mainG", "coudeG", "mainD", "coudeD"):
        x, y = q[nom]
        plume(t, x, y + 1, 100, couleur=(240, 235, 220), long=7)
    for y in (14, 24, 34):
        t.peindre(t.masque(lambda d, y=y: d.arc([24, y - 5, 48, y + 5], 270, 360, fill=1, width=1)), BLANC)
    for x, y, a in ((30, 12, -10), (38, 22, 5), (32, 32, 10)):
        plume(t, x, y, a, couleur=(240, 240, 230), long=9)
    t.vignette(0.4)
    return t


def forestWolf():
    """Nuit bleue de pleine lune : le loup se dresse dans une gerbe de feuilles"""
    t = Toile()
    t.fond_radial(32, 12, 44, [(220, 230, 255), (90, 110, 180), (30, 40, 90), (8, 12, 30)])
    orbe(t, 34, 11, 6, (240, 240, 220))
    sol = t.masque(lambda d: d.polygon([(0, 40), (20, 34), (48, 38), (48, 48), (0, 48)], fill=1))
    t.peindre(sol, (14, 20, 30))
    bete(t, "loup", 22, 29, s=1.1, couleur=(24, 28, 44), liseres=(((120, 140, 200), 0, -1),))
    feuilles(t, ((6, 14, 30), (12, 6, 120), (42, 30, 200)), couleur=(90, 160, 80))
    t.vignette(0.35)
    return t


def runicBear():
    """Bleu runique : l'ours se dresse sur un sceau qui s'allume"""
    t = Toile()
    t.fond_radial(24, 34, 42, [(120, 200, 230), (40, 110, 160), (14, 50, 80), (4, 18, 30)])
    anneau(t, 24, 40, 20, 5, (140, 240, 255), 2, clair=BLANC)
    for a in range(0, 360, 45):
        x, y = 24 + 16 * math.cos(math.radians(a)), 40 + 4 * math.sin(math.radians(a))
        t.point(x, y, BLANC)
    t.lueur(24, 38, 16, (140, 240, 255), 0.5, sy=0.6)
    bete(t, "ours", 24, 27, s=1.1, couleur=(40, 30, 30), liseres=(((160, 240, 255), 0, -1),))
    for x, y in ((17, 24), (23, 22), (29, 25)):
        t.peindre(t.masque(lambda d, x=x, y=y: d.line([(x, y), (x + 1, y + 3)], fill=1)), (140, 240, 255))
    t.vignette(0.4)
    return t


def ancientStag():
    """Aube brumeuse : le grand cerf, un cercle de régénération à ses pieds"""
    t = Toile()
    t.fond_radial(24, 20, 44, [(240, 255, 220), (160, 220, 160), (70, 140, 100), (20, 60, 40), (6, 24, 16)])
    anneau(t, 22, 40, 18, 4, (140, 255, 140), 2, clair=BLANC)
    t.lueur(22, 38, 14, (160, 255, 160), 0.5, sy=0.5)
    bete(t, "cerf", 20, 28, s=1.05, couleur=(40, 36, 30), liseres=((BLANC, 0, -1),))
    for x, y in ((8, 30), (38, 32), (30, 42)):
        t.etincelle(x, y, (180, 255, 170))
    t.vignette(0.4)
    return t


def peregrineFalcon():
    """Couchant orange : une explosion de plumes, le faucon prend son envol"""
    t = Toile()
    t.fond_radial(30, 18, 44, [(255, 230, 160), (250, 150, 60), (190, 70, 40), (90, 24, 30), (30, 8, 14)])
    for x, y, a in ((6, 38, -60), (14, 42, -100), (4, 28, -30), (20, 38, -80)):
        plume(t, x, y, a, couleur=(240, 220, 190), long=9)
    bete(t, "oiseau", 28, 20, s=1.3, couleur=(40, 24, 24), liseres=(((255, 220, 150), 0, -1),))
    t.point(28, 7, (255, 200, 40))
    t.vignette(0.4)
    return t


def owlbear():
    """Ocre : l'ours-hibou dressé retombe d'un bond, l'onde de choc court au sol"""
    t = Toile()
    t.fond_vertical([(160, 110, 40), (220, 170, 80), (240, 210, 130)], 0, 38)
    sol = t.masque(lambda d: d.rectangle([0, 38, 48, 48], fill=1))
    t.zone_verticale(sol, [(120, 80, 40), (50, 30, 14)], 38, 48)
    anneau(t, 24, 41, 22, 4, (255, 230, 160), 1)
    corps = t.masque(lambda d: (d.ellipse([12, 14, 36, 40], fill=1),
                                d.ellipse([14, 4, 34, 22], fill=1),
                                d.polygon([(15, 8), (13, 0), (19, 6)], fill=1), d.polygon([(33, 8), (35, 0), (29, 6)], fill=1),
                                d.line([(14, 22), (6, 16), (4, 10)], fill=1, width=4), d.line([(34, 22), (42, 16), (44, 10)], fill=1, width=4),
                                d.line([(18, 36), (17, 42)], fill=1, width=5), d.line([(30, 36), (31, 42)], fill=1, width=5)))
    t.objet(corps, (110, 74, 40), (170, 130, 80), (60, 36, 18))
    visage = t.masque(lambda d: d.ellipse([17, 7, 31, 20], fill=1))
    t.peindre(visage, (200, 170, 120))
    for x in (21, 27):
        t.peindre(t.masque(lambda d, x=x: d.ellipse([x - 2, 10, x + 2, 14], fill=1)), (255, 200, 40))
        t.point(x, 12, (20, 10, 10))
    t.peindre(t.masque(lambda d: d.polygon([(22, 15), (26, 15), (24, 19)], fill=1)), (60, 40, 20))
    t.vignette(0.4)
    return t


def wendigo():
    """Gris glacé : une créature décharnée au crâne de cerf et aux bois acérés, dans la brume"""
    t = Toile()
    t.fond_radial(24, 20, 42, [(200, 210, 220), (120, 130, 150), (60, 64, 80), (18, 20, 28)])
    nuage(t, 24, 44, 48, 8, (170, 180, 190), graine=9, alpha=0.6)
    pose = {"tete": (24, 13), "cou": (24, 17), "hanche": (24, 29), "coudeG": (15, 22), "mainG": (11, 32),
            "coudeD": (33, 22), "mainD": (37, 32), "genouG": (19, 35), "piedG": (21, 44), "genouD": (29, 35), "piedD": (27, 44)}
    corps = t.masque(lambda d: silhouette(d, pose, 0.8))
    t.peindre(corps, (24, 20, 26))
    t.liseré(corps, (200, 210, 230), 0, -1)
    for c in (-1, 1):
        griffes = t.masque(lambda d, c=c: [d.line([(24 + c * 13, 32), (24 + c * (12 + i), 37)], fill=1) for i in range(3)])
        t.peindre(griffes, (220, 220, 210))
    crane(t, 24, 12, 3.2, os=(230, 225, 205), yeux=(255, 40, 30))
    bois_m = t.masque(lambda d: [(d.line([(24 + b * 2, 9), (24 + b * 7, 3), (24 + b * 14, 1)], fill=1, width=2),
                                  d.line([(24 + b * 6, 4), (24 + b * 5, 0)], fill=1),
                                  d.line([(24 + b * 10, 2), (24 + b * 12, 7)], fill=1)) for b in (-1, 1)])
    t.objet(bois_m, (220, 210, 190), BLANC, (140, 130, 110), contour=(30, 26, 30))
    sang(t, ((10, 36, 1.5), (38, 36, 1.5)))
    t.vignette(0.45)
    return t


def sylvanGriffin():
    """Or et vert : le griffon sylvestre, tête d'aigle et corps de bête, déploie ses ailes"""
    t = Toile()
    t.fond_radial(24, 18, 44, [(250, 250, 190), (190, 220, 110), (100, 150, 60), (40, 70, 30), (14, 28, 10)])
    bete(t, "griffon", 26, 30, s=1.2, couleur=(70, 50, 30), liseres=(((255, 240, 170), 0, -1),))
    t.point(38, 13, (255, 220, 60))
    for x, y, a in ((6, 38, -40), (42, 40, -120)):
        plume(t, x, y, a, couleur=(230, 240, 180), long=8)
    t.vignette(0.4)
    return t


def voidAvatar():
    """Ultime : une faille d'Étherium, le druide en ressort en chimère ailée constellée de cristaux"""
    t = Toile()
    t.fond_radial(24, 26, 42, [(100, 50, 160), (50, 16, 100), (20, 6, 44), (6, 2, 14)])
    col = t.masque(lambda d: d.rectangle([14, 0, 34, 42], fill=1))
    t.voile(col, (150, 90, 230), 0.3)
    fissure(t, 4, 44, 44, 43)
    ailes = t.masque(lambda d: (d.polygon([(18, 22), (4, 6), (10, 4), (22, 18)], fill=1), d.polygon([(26, 20), (30, 2), (36, 6), (30, 22)], fill=1)))
    t.peindre(ailes, (60, 30, 100))
    t.liseré(ailes, CYAN, 0, -1)
    bete(t, "ours", 22, 32, s=1.05, couleur=(70, 36, 110), liseres=((CYAN, 0, -1), ((40, 16, 70), 0, 1)))
    cristaux(t, ((14, 24, 5), (20, 21, 6), (26, 23, 5)))
    t.vignette(0.35)
    return t

