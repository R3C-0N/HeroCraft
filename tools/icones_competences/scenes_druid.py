"""Scènes du Druide : métamorphoses, forêt et bêtes, chaque forme sa saison."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater

FEUILLE = (120, 200, 70)
FEUILLE_S = (60, 130, 40)


def feuilles(t, pts, couleur=FEUILLE):
    for x, y, a in pts:
        plume(t, x, y, a, couleur=couleur, long=5)


def packClaws():
    """Vert de sous-bois : des feuilles tourbillonnent, trois griffures lacèrent l'air"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(170, 230, 120), (70, 150, 60), (24, 80, 30), (8, 30, 12)])
    griffures(t, 24, 24, couleur=(255, 250, 230), sombre=(150, 20, 30), long=30, angle=-55, ecart=7)
    feuilles(t, ((6, 10, 30), (40, 8, 120), (8, 38, -40), (40, 38, 200), (14, 20, 80)))
    t.vignette(0.4)
    return t


def bearHide():
    """Ambre chaud : une tête d'ours dans une gerbe de poils, un halo brun"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(250, 210, 140), (210, 140, 60), (130, 70, 30), (50, 24, 10)])
    t.lueur(24, 26, 18, (200, 130, 60), 0.6)
    tete = t.masque(lambda d: (d.ellipse([10, 12, 38, 40], fill=1), d.ellipse([8, 8, 17, 17], fill=1), d.ellipse([31, 8, 40, 17], fill=1)))
    t.objet(tete, (110, 70, 40), (170, 120, 70), (60, 34, 20))
    museau = t.masque(lambda d: d.ellipse([17, 26, 31, 38], fill=1))
    t.objet(museau, (170, 130, 90), (210, 170, 120), (110, 80, 50))
    t.peindre(t.masque(lambda d: d.ellipse([21, 27, 27, 31], fill=1)), (20, 10, 10))
    t.point(17, 21, (20, 10, 10)); t.point(30, 21, (20, 10, 10))
    for x, y in ((6, 30), (42, 28), (24, 6), (6, 20)):
        t.peindre(t.masque(lambda d, x=x, y=y: d.line([(x, y), (x + 2, y - 3)], fill=1)), (150, 100, 50))
    t.vignette(0.4)
    return t


def elkLegs():
    """Fauve des plaines : un coup de sabot soulève la poussière"""
    t = Toile()
    t.fond_vertical([(200, 170, 110), (230, 200, 140), (180, 140, 90)], 0, 36)
    sol = t.masque(lambda d: d.rectangle([0, 36, 48, 48], fill=1))
    t.zone_verticale(sol, [(140, 100, 60), (70, 46, 26)], 36, 48)
    jambe = t.masque(lambda d: (d.line([(30, 2), (26, 18), (28, 28)], fill=1, width=5), d.line([(28, 28), (22, 34)], fill=1, width=3)))
    t.objet(jambe, (150, 100, 60), (200, 150, 100), (80, 50, 30))
    sabot = t.masque(lambda d: d.polygon([(17, 33), (24, 31), (26, 37), (18, 38)], fill=1))
    t.objet(sabot, (50, 36, 30), (90, 70, 60), (20, 14, 10))
    nuage(t, 14, 38, 22, 8, (220, 190, 140), graine=6)
    nuage(t, 34, 40, 16, 6, (210, 180, 130), graine=8)
    t.vignette(0.4)
    return t


def galeFeathers():
    """Bleu-vert du large : un battement projette des plumes et une rafale"""
    t = Toile()
    t.fond_radial(18, 24, 44, [(220, 255, 250), (110, 210, 210), (40, 130, 150), (14, 60, 80), (4, 24, 34)])
    for y, l in ((14, 16), (24, 22), (34, 14)):
        t.peindre(t.masque(lambda d, y=y, l=l: d.arc([18, y - 4, 18 + l * 2, y + 4], 270, 360, fill=1, width=1)), BLANC)
    heros(t, "brasOuverts", 12, 28, s=0.8, liseres=((BLANC, 1, 0),))
    for x, y, a in ((30, 10, 10), (38, 18, -20), (28, 26, 0), (40, 30, 15), (34, 38, -10)):
        plume(t, x, y, a, couleur=(240, 240, 230))
    t.vignette(0.4)
    return t


def forestWolf():
    """Nuit bleue de pleine lune : le loup se dresse dans une gerbe de feuilles"""
    t = Toile()
    t.fond_radial(32, 12, 44, [(220, 230, 255), (90, 110, 180), (30, 40, 90), (8, 12, 30)])
    orbe(t, 34, 11, 6, (240, 240, 220))
    sol = t.masque(lambda d: d.polygon([(0, 40), (20, 34), (48, 38), (48, 48), (0, 48)], fill=1))
    t.peindre(sol, (14, 20, 30))
    bete(t, "loup", 22, 29, s=1.1, couleur=(24, 28, 44), liseres=(((200, 210, 255), 0, -1),))
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
    """Couchant orange : une explosion de plumes, le faucon fond en piqué"""
    t = Toile()
    t.fond_radial(30, 18, 44, [(255, 230, 160), (250, 150, 60), (190, 70, 40), (90, 24, 30), (30, 8, 14)])
    for x, y, a in ((8, 38, 30), (14, 44, -20), (4, 30, 60), (18, 36, 0)):
        plume(t, x, y, a, couleur=(240, 220, 190))
    bete(t, "oiseau", 26, 20, s=1.3, couleur=(40, 24, 24), liseres=(((255, 220, 150), 0, -1),))
    t.vignette(0.4)
    return t


def owlbear():
    """Ocre : l'ours-hibou retombe d'un bond dans une onde de choc"""
    t = Toile()
    t.fond_vertical([(160, 110, 40), (220, 170, 80), (240, 210, 130)], 0, 36)
    sol = t.masque(lambda d: d.rectangle([0, 36, 48, 48], fill=1))
    t.zone_verticale(sol, [(120, 80, 40), (50, 30, 14)], 36, 48)
    anneau(t, 24, 39, 22, 4, (255, 230, 160), 1)
    anneau(t, 24, 39, 15, 3, (255, 240, 200), 1)
    nuage(t, 8, 37, 12, 6, (210, 180, 120), graine=2)
    nuage(t, 40, 37, 12, 6, (210, 180, 120), graine=3)
    bete(t, "hibours", 22, 27, s=1.1, couleur=(60, 40, 24), liseres=(((255, 220, 150), 0, -1),))
    t.point(35, 21, (255, 200, 40))
    t.vignette(0.4)
    return t


def wendigo():
    """Gris glacé : une forme sombre aux bois acérés, dans la brume, du sang à chaque touche"""
    t = Toile()
    t.fond_radial(24, 20, 42, [(200, 210, 220), (120, 130, 150), (60, 64, 80), (18, 20, 28)])
    nuage(t, 24, 42, 48, 10, (170, 180, 190), graine=9, alpha=0.6)
    heros(t, "brasOuverts", 24, 28, s=1.05, couleur=(20, 18, 24), liseres=(((210, 220, 240), 0, -1),))
    bois_m = t.masque(lambda d: [(d.line([(24 + b * 2, 12), (24 + b * 7, 4), (24 + b * 12, 2)], fill=1, width=1),
                                  d.line([(24 + b * 6, 6), (24 + b * 6, 1)], fill=1)) for b in (-1, 1)])
    t.peindre(bois_m, (200, 190, 170))
    t.point(23, 13, (255, 40, 30)); t.point(26, 13, (255, 40, 30))
    sang(t, ((10, 20, 1.5), (38, 18, 1.5), (40, 26, 1)))
    t.vignette(0.45)
    return t


def sylvanGriffin():
    """Or et vert : le griffon sylvestre déploie ses ailes, des plumes volent"""
    t = Toile()
    t.fond_radial(24, 18, 44, [(250, 250, 190), (190, 220, 110), (100, 150, 60), (40, 70, 30), (14, 28, 10)])
    bete(t, "griffon", 24, 24, s=1.25, couleur=(70, 50, 30), liseres=(((255, 240, 170), 0, -1),))
    for x, y, a in ((6, 36, 20), (40, 40, -30), (42, 10, 100)):
        plume(t, x, y, a, couleur=(230, 240, 180))
    t.vignette(0.4)
    return t


def voidAvatar():
    """Ultime : une faille d'Étherium, le druide en ressort en chimère constellée de cristaux"""
    t = Toile()
    t.fond_radial(24, 26, 42, [(100, 50, 160), (50, 16, 100), (20, 6, 44), (6, 2, 14)])
    col = t.masque(lambda d: d.rectangle([16, 0, 32, 42], fill=1))
    t.voile(col, (150, 90, 230), 0.35)
    fissure(t, 6, 43, 42, 42)
    bete(t, "ours", 20, 30, s=1.05, couleur=(30, 14, 46), liseres=((CYAN, 0, -1),))
    bete(t, "oiseau", 24, 12, s=0.8, couleur=(30, 14, 46), liseres=((CYAN, 0, -1),))
    cristaux(t, ((20, 22, 6), (28, 20, 7), (34, 24, 5)))
    t.vignette(0.35)
    return t
