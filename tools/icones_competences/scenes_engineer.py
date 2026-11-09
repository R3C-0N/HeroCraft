"""Scènes de l'Ingénieur : tourelles, pièges et cuivre, l'atelier sur le champ de bataille."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater

CUIVRE = (200, 120, 60)
CUIVRE_C = (250, 190, 120)
FER = (110, 110, 120)


def tourelle(t, cx, sol, canon=(1, 0), couleur=CUIVRE, flamme=False):
    pied = t.masque(lambda d: d.polygon([(cx - 8, sol), (cx - 3, sol - 8), (cx + 3, sol - 8), (cx + 8, sol)], fill=1))
    t.objet(pied, FER, (170, 170, 180), (50, 50, 60))
    corps = t.masque(lambda d: d.rounded_rectangle([cx - 6, sol - 16, cx + 6, sol - 7], 2, fill=1))
    t.objet(corps, couleur, CUIVRE_C, melange(couleur, (0, 0, 0), 0.5))
    cx2, cy2 = cx + canon[0] * 12, sol - 12 + canon[1] * 12
    tube = t.masque(lambda d: d.line([(cx, sol - 12), (cx2, cy2)], fill=1, width=3 if not flamme else 4))
    t.objet(tube, FER, (170, 170, 180), (40, 40, 50))
    t.point(cx - 2, sol - 13, (255, 220, 120))
    return cx2, cy2


def engrenage(t, cx, cy, r, couleur=CUIVRE):
    m = t.masque(lambda d: (d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=1),
                            [d.rectangle([cx + (r + 1) * math.cos(math.radians(a)) - 1.5, cy + (r + 1) * math.sin(math.radians(a)) - 1.5,
                                          cx + (r + 1) * math.cos(math.radians(a)) + 1.5, cy + (r + 1) * math.sin(math.radians(a)) + 1.5],
                                         fill=1) for a in range(0, 360, 45)]))
    t.objet(m, couleur, CUIVRE_C, melange(couleur, (0, 0, 0), 0.5))
    t.peindre(t.masque(lambda d: d.ellipse([cx - r * 0.35, cy - r * 0.35, cx + r * 0.35, cy + r * 0.35], fill=1)), (30, 20, 16))


def ballisticTurret():
    """Ciel de poussière : la tourelle se pose et tire ses carreaux"""
    t = Toile()
    t.fond_vertical([(120, 150, 170), (210, 190, 150), (190, 150, 100)], 0, 38)
    sol = t.masque(lambda d: d.rectangle([0, 38, 48, 48], fill=1))
    t.zone_verticale(sol, [(130, 100, 70), (60, 44, 30)], 38, 48)
    nuage(t, 16, 38, 28, 6, (220, 200, 160), graine=2)
    x, y = tourelle(t, 16, 38, canon=(1, -0.3))
    for i in range(2):
        fleche(t, x + 4 + i * 10, y - 1 - i * 3, x + 10 + i * 10, y - 3 - i * 3, fut=(90, 70, 60))
    t.vignette(0.4)
    return t


def jawTrap():
    """Rouge de sous-bois : le piège claque et fait gicler le sang"""
    t = Toile()
    t.fond_radial(24, 30, 42, [(200, 140, 100), (130, 60, 40), (60, 20, 16), (20, 6, 4)])
    for c in (-1, 1):
        dents = t.masque(lambda d, c=c: d.polygon([(8, 34), (40, 34)] + [(40 - i * 4, 34 + c * (8 if i % 2 else 3)) for i in range(9)], fill=1))
        t.objet(dents, (190, 190, 200), BLANC, (90, 90, 100))
    anneau(t, 24, 36, 18, 4, FER, 2)
    sang(t, ((20, 16, 2), (28, 12, 2.5), (34, 20, 1.5), (14, 22, 1.5)))
    t.vignette(0.4)
    return t


def shrapnelMine():
    """Ciel orange de champ de mines : la mine saute en projetant des éclats"""
    t = Toile()
    t.fond_radial(24, 30, 44, [(255, 230, 160), (240, 140, 60), (160, 60, 30), (60, 20, 10)])
    explosion(t, 24, 30, 12)
    for a in range(-160, -10, 25):
        r = math.radians(a)
        t.peindre(t.masque(lambda d, r=r: d.line([(24 + 13 * math.cos(r), 30 + 13 * math.sin(r)),
                                                   (24 + 20 * math.cos(r), 30 + 20 * math.sin(r))], fill=1, width=2)), (200, 200, 210))
    mine = t.masque(lambda d: d.chord([16, 34, 32, 46], 180, 360, fill=1))
    t.objet(mine, FER, (170, 170, 180), (40, 40, 50))
    t.vignette(0.4)
    return t


def rescueGrapple():
    """Bleu de falaise : un câble lancé tire l'ingénieur à l'abri"""
    t = Toile()
    t.fond_vertical([(160, 200, 230), (90, 140, 190), (40, 70, 110)], 0, 48)
    roc = t.masque(lambda d: d.polygon([(32, 4), (48, 0), (48, 48), (36, 48), (30, 24)], fill=1))
    t.objet(roc, (100, 90, 90), (150, 140, 140), (50, 44, 44))
    t.peindre(t.masque(lambda d: d.line([(12, 34), (33, 12)], fill=1)), (230, 220, 200))
    crochet = t.masque(lambda d: d.arc([31, 8, 37, 14], 180, 90, fill=1, width=2))
    t.peindre(crochet, (200, 200, 210))
    heros(t, "saut", 12, 34, s=0.75, couleur=(30, 24, 30), liseres=((CUIVRE_C, 1, 0),))
    t.vignette(0.4)
    return t


def flameTurret():
    """Rouge forge : un petit canon crache un cône de flammes"""
    t = Toile()
    t.fond_radial(32, 26, 44, [(255, 220, 140), (230, 100, 40), (140, 30, 20), (50, 10, 6)])
    sol = t.masque(lambda d: d.rectangle([0, 40, 48, 48], fill=1))
    t.peindre(sol, (40, 20, 14))
    x, y = tourelle(t, 12, 40, canon=(1, -0.1), flamme=True)
    cone = t.masque(lambda d: d.polygon([(x, y), (46, y - 12), (46, y + 10)], fill=1))
    t.objet(cone, (255, 140, 30), (255, 240, 160), (220, 60, 20), contour=(100, 20, 10))
    t.peindre(t.masque(lambda d: d.polygon([(x + 2, y), (44, y - 6), (44, y + 5)], fill=1)), (255, 220, 100))
    t.vignette(0.4)
    return t


def magneticTrap():
    """Cuivre et bleu : un cercle au sol aspire les ennemis vers son centre"""
    t = Toile()
    t.fond_radial(24, 30, 42, [(150, 200, 220), (60, 110, 150), (20, 46, 70), (6, 16, 26)])
    anneau(t, 24, 36, 20, 6, CUIVRE, 2, clair=CUIVRE_C)
    anneau(t, 24, 36, 12, 3.5, CUIVRE, 2)
    engrenage(t, 24, 35, 3)
    for c in (-1, 1):
        t.peindre(t.masque(lambda d, c=c: d.arc([24 - 18, 14, 24 + 18, 44], 200 if c < 0 else 300, 240 if c < 0 else 340, fill=1, width=1)), (150, 230, 255))
        heros(t, "tombe", 24 + c * 16, 26, s=0.5, couleur=(20, 24, 30), liseres=(((150, 230, 255), -c, 0),), retourne=c > 0)
    t.vignette(0.4)
    return t


def portableBarricade():
    """Gris de chantier : le module se déplie en mur dans un nuage de poussière"""
    t = Toile()
    t.fond_vertical([(140, 160, 180), (190, 190, 180), (170, 150, 120)], 0, 38)
    sol = t.masque(lambda d: d.rectangle([0, 38, 48, 48], fill=1))
    t.zone_verticale(sol, [(120, 100, 80), (50, 40, 30)], 38, 48)
    for i, x in enumerate((8, 18, 28)):
        plaque = t.masque(lambda d, x=x: d.rectangle([x, 14, x + 9, 38], fill=1))
        t.objet(plaque, (150, 150, 160), (210, 210, 220), (70, 70, 80))
        for y in (18, 34):
            t.point(x + 2, y, CUIVRE_C); t.point(x + 7, y, CUIVRE_C)
    nuage(t, 24, 40, 44, 7, (210, 200, 180), graine=3)
    t.vignette(0.4)
    return t


def mechanicalOverload():
    """Bleu électrique : un signal part de la main vers chaque tourelle, qui s'illumine"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(180, 230, 255), (70, 140, 220), (24, 56, 120), (6, 16, 40)])
    tourelle(t, 36, 22, couleur=CUIVRE)
    tourelle(t, 36, 44, couleur=CUIVRE)
    t.lueur(36, 12, 7, (200, 240, 255), 0.7)
    t.lueur(36, 34, 7, (200, 240, 255), 0.7)
    eclair(t, zigzag(10, 28, 32, 12, n=4, amp=2, graine=5), couleur=(140, 220, 255))
    eclair(t, zigzag(10, 28, 32, 34, n=4, amp=2, graine=6), couleur=(140, 220, 255))
    main = t.masque(lambda d: (d.ellipse([3, 24, 13, 34], fill=1), d.rectangle([2, 30, 8, 42], fill=1)))
    t.objet(main, (150, 120, 90), (200, 170, 130), (90, 70, 50))
    t.vignette(0.4)
    return t


def executionZone():
    """Orange de cible : les tirs convergent vers le centre du piège"""
    t = Toile()
    t.fond_radial(24, 26, 42, [(255, 210, 150), (230, 120, 50), (140, 50, 20), (50, 16, 6)])
    mire(t, 24, 28, 10, couleur=(255, 240, 200))
    anneau(t, 24, 28, 4, 4, (255, 240, 200), 1)
    for x0, y0 in ((2, 6), (46, 6), (2, 46), (46, 46)):
        t.peindre(t.masque(lambda d, x0=x0, y0=y0: d.line([(x0, y0), (24 + (x0 - 24) * 0.35, 28 + (y0 - 28) * 0.35)], fill=1, width=2)),
                  (255, 160, 40))
    t.etincelle(24, 28, (255, 220, 120), grande=True)
    t.vignette(0.4)
    return t


def furnaceWall():
    """Braise : la barricade rougeoie et lèche de flammes"""
    t = Toile()
    t.fond_vertical([(30, 10, 10), (100, 30, 16), (180, 70, 24)], 0, 40)
    for i, x in enumerate((8, 18, 28)):
        plaque = t.masque(lambda d, x=x: d.rectangle([x, 16, x + 9, 40], fill=1))
        t.objet(plaque, (170, 60, 30), (255, 160, 80), (90, 24, 10))
    for x in (10, 20, 30, 38):
        flamme = t.masque(lambda d, x=x: d.polygon([(x - 3, 18), (x, 6 + (x % 3) * 2), (x + 3, 18)], fill=1))
        t.objet(flamme, (255, 160, 40), (255, 240, 160), (230, 70, 20), contour=(100, 20, 6))
    t.lueur(24, 28, 18, (255, 120, 40), 0.4)
    t.vignette(0.4)
    return t


def targetedEradication():
    """Rouge alarme : trois mines tombent autour de l'ennemi pris au piège"""
    t = Toile()
    t.fond_radial(24, 28, 42, [(255, 170, 140), (210, 60, 50), (110, 16, 24), (40, 4, 8)])
    heros(t, "accroupi", 24, 30, s=0.8, couleur=(30, 10, 14), liseres=((BLANC, 0, -1),))
    anneau(t, 24, 40, 16, 4, CUIVRE, 1)
    for x, y in ((8, 12), (24, 4), (40, 12)):
        mine = t.masque(lambda d, x=x, y=y: d.ellipse([x - 3, y - 3, x + 3, y + 3], fill=1))
        t.objet(mine, FER, (170, 170, 180), (40, 40, 50))
        t.point(x, y - 1, (255, 60, 40))
        t.peindre(t.masque(lambda d, x=x, y=y: d.line([(x, y - 9), (x, y - 5)], fill=1)), (255, 220, 200))
    t.vignette(0.4)
    return t


def etheriumBastion():
    """Ultime : le sol s'ouvre, la forteresse s'assemble, les obus éclatent en cyan"""
    t = Toile()
    t.fond_radial(24, 26, 42, [(100, 50, 170), (50, 16, 100), (20, 6, 44), (6, 2, 14)])
    fissure(t, 2, 44, 46, 43)
    for x in (6, 34):
        mur(t, x, 18, x + 8, 42, pierre=(80, 60, 100), joint=(200, 120, 60))
    mur(t, 14, 24, 34, 42, pierre=(90, 70, 110), joint=CYAN)
    for x, y in ((10, 8), (24, 6), (38, 10)):
        explosion(t, x, y, 4, couleurs=(BLANC, CYAN, (120, 80, 220)), branches=6)
    t.vignette(0.3)
    return t
