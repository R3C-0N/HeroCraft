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
    """Bleu de falaise : un câble tendu tire l'ingénieur vers la corniche"""
    t = Toile()
    t.fond_vertical([(160, 200, 230), (90, 140, 190), (40, 70, 110)], 0, 48)
    sol = t.masque(lambda d: d.rectangle([0, 42, 48, 48], fill=1))
    t.zone_verticale(sol, [(80, 70, 70), (30, 26, 26)], 42, 48)
    roc = t.masque(lambda d: d.polygon([(30, 12), (36, 8), (48, 6), (48, 48), (34, 48), (36, 30), (32, 22)], fill=1))
    t.objet(roc, (110, 100, 96), (170, 160, 150), (50, 44, 44))
    t.peindre(t.masque(lambda d: d.line([(31, 12), (48, 7)], fill=1)), (200, 230, 200))
    corps, q = heros(t, "brasTendu", 13, 32, s=0.85, couleur=(30, 24, 30), liseres=((CUIVRE_C, 1, 0),))
    mx, my = q["mainD"]
    t.peindre(t.masque(lambda d: d.line([(mx, my), (33, 12)], fill=1)), (240, 230, 210))
    t.peindre(t.masque(lambda d: d.line([(32, 13), (35, 9)], fill=1, width=2)), (200, 200, 210))
    t.peindre(t.masque(lambda d: d.arc([33, 7, 38, 12], 180, 360, fill=1)), (220, 220, 230))
    for i, y in enumerate((30, 34, 38)):
        t.peindre(t.masque(lambda d, y=y, i=i: d.line([(2 + i, y), (7 + i, y)], fill=1)), BLANC)
    nuage(t, 8, 43, 14, 5, (200, 190, 180), graine=2)
    t.vignette(0.4)
    return t


def flameTurret():
    """Rouge forge : un petit canon crache un cône de flammes"""
    t = Toile()
    t.fond_radial(32, 26, 44, [(255, 220, 140), (230, 100, 40), (140, 30, 20), (50, 10, 6)])
    sol = t.masque(lambda d: d.rectangle([0, 40, 48, 48], fill=1))
    t.peindre(sol, (40, 20, 14))
    x, y = tourelle(t, 11, 40, canon=(1, -0.1), flamme=True)
    t.lueur(36, y, 12, (255, 200, 80), 0.6)
    pts = [(x, y)]
    for i in range(9):
        a = math.radians(-24 + i * 6)
        r = 22 + (5 if i % 2 else 0) + (i % 3)
        pts.append((x + r * math.cos(a), y + r * math.sin(a)))
    pts.append((x, y))
    cone = t.masque(lambda d: d.polygon(pts, fill=1))
    t.objet(cone, (255, 140, 30), (255, 240, 160), (220, 60, 20), contour=(100, 20, 10))
    coeur = [(x + (px - x) * 0.6, y + (py - y) * 0.5) for px, py in pts]
    t.peindre(t.masque(lambda d: d.polygon(coeur, fill=1)), (255, 230, 120))
    t.peindre(t.masque(lambda d: d.polygon([(x + 1, y), (x + 8, y - 1), (x + 8, y + 1)], fill=1)), BLANC)
    t.vignette(0.4)
    return t


def magneticTrap():
    """Cuivre et bleu : un cercle au sol aspire les ennemis vers son centre"""
    t = Toile()
    t.fond_radial(24, 30, 42, [(150, 200, 220), (60, 110, 150), (20, 46, 70), (6, 16, 26)])
    anneau(t, 24, 38, 20, 6, CUIVRE, 2, clair=CUIVRE_C)
    anneau(t, 24, 38, 12, 3.5, CUIVRE, 2)
    engrenage(t, 24, 37, 3)
    for c in (-1, 1):
        heros(t, "course", 24 + c * 14, 30, s=0.75, couleur=(26, 34, 50), liseres=(((170, 240, 255), 0, -1),), retourne=c > 0)
        for k in range(3):
            xx = 24 + c * (22 - k * 3)
            t.peindre(t.masque(lambda d, xx=xx, k=k: d.line([(xx, 22 + k * 3), (xx + c * 3, 22 + k * 3)], fill=1)), (170, 240, 255))
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
    """Bleu électrique : l'ingénieur lance une décharge, ses deux tourelles s'illuminent"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(180, 230, 255), (70, 140, 220), (24, 56, 120), (6, 16, 40)])
    sol = t.masque(lambda d: d.rectangle([0, 42, 48, 48], fill=1))
    t.peindre(sol, (20, 30, 50))
    corps, q = heros(t, "brasLeves", 10, 32, s=0.9, couleur=(24, 20, 30), liseres=(((140, 220, 255), 1, 0),))
    mx, my = q["mainD"]
    for cx, sol_y in ((30, 42), (42, 30)):
        t.lueur(cx, sol_y - 12, 7, (200, 240, 255), 0.7)
    tourelle(t, 30, 42, canon=(-0.2, -1))
    pied = t.masque(lambda d: d.rectangle([36, 30, 48, 32], fill=1))
    t.objet(pied, FER, (170, 170, 180), (50, 50, 60))
    tourelle(t, 42, 30, canon=(-1, -0.3))
    eclair(t, zigzag(mx, my, 28, 26, n=4, amp=2, graine=5), couleur=(140, 220, 255))
    eclair(t, zigzag(mx, my, 40, 15, n=4, amp=2, graine=6), couleur=(140, 220, 255))
    t.vignette(0.4)
    return t


def executionZone():
    """Orange de cible : la tourelle arrose le piège où l'ennemi est pris"""
    t = Toile()
    t.fond_radial(28, 26, 42, [(255, 210, 150), (230, 120, 50), (140, 50, 20), (50, 16, 6)])
    sol = t.masque(lambda d: d.rectangle([0, 40, 48, 48], fill=1))
    t.peindre(sol, (60, 24, 10))
    anneau(t, 32, 40, 13, 3.5, (255, 240, 200), 1)
    for c in (-1, 1):
        dents = t.masque(lambda d, c=c: d.polygon([(23, 40)] + [(23 + i * 3, 40 + c * (3 if i % 2 else 1)) for i in range(7)] + [(41, 40)], fill=1))
        t.objet(dents, (190, 190, 200), BLANC, (90, 90, 100))
    heros(t, "accroupi", 32, 32, s=0.75, couleur=(40, 14, 10), liseres=((BLANC, 0, -1),), retourne=True)
    mire(t, 32, 22, 6, couleur=(255, 240, 200))
    x, y = tourelle(t, 8, 40, canon=(1, -0.35))
    for i, (x0, y0) in enumerate(((x + 2, y - 1), (x + 8, y - 2))):
        t.peindre(t.masque(lambda d, x0=x0, y0=y0: d.line([(x0, y0), (x0 + 5, y0 - 1)], fill=1, width=2)), (255, 240, 160))
    t.etincelle(32, 28, (255, 220, 120), grande=True)
    t.vignette(0.4)
    return t


def furnaceWall():
    """Braise : la barricade rougeoie et lèche de flammes"""
    t = Toile()
    t.fond_vertical([(30, 10, 10), (100, 30, 16), (180, 70, 24)], 0, 40)
    sol = t.masque(lambda d: d.rectangle([0, 40, 48, 48], fill=1))
    t.peindre(sol, (40, 14, 8))
    t.lueur(24, 24, 18, (255, 120, 40), 0.5)
    for i, (x, h, w, p) in enumerate(((9, 14, 3.5, -2), (17, 22, 4, 2), (25, 17, 3.5, -1), (33, 24, 4.5, 3), (40, 12, 3, -2))):
        flamme(t, x, 22, h, w, penche=p, graine=i + 2)
    for x in (6, 17, 28):
        plaque = t.masque(lambda d, x=x: d.rectangle([x, 20, x + 10, 40], fill=1))
        t.objet(plaque, (170, 60, 30), (255, 160, 80), (90, 24, 10))
        t.peindre(t.masque(lambda d, x=x: d.line([(x + 2, 30), (x + 8, 30)], fill=1)), (255, 220, 120))
    t.vignette(0.4)
    return t


def targetedEradication():
    """Rouge alarme : trois mines tombent en sifflant autour de l'ennemi pris au piège"""
    t = Toile()
    t.fond_radial(24, 28, 42, [(255, 170, 140), (210, 60, 50), (110, 16, 24), (40, 4, 8)])
    sol = t.masque(lambda d: d.rectangle([0, 42, 48, 48], fill=1))
    t.peindre(sol, (50, 10, 14))
    anneau(t, 24, 42, 17, 4, CUIVRE, 1, clair=CUIVRE_C)
    heros(t, "debout", 24, 34, s=0.85, couleur=(30, 10, 14), liseres=((BLANC, 0, -1),))
    for x, y in ((8, 26), (40, 22), (24, 8)):
        for k in range(3):
            t.point(x - 1 - k, y - 6 - k * 3, (255, 210, 190))
            t.point(x + 1 + k, y - 6 - k * 3, (255, 210, 190))
        mine = t.masque(lambda d, x=x, y=y: d.ellipse([x - 4, y - 3, x + 4, y + 3], fill=1))
        t.objet(mine, FER, (170, 170, 180), (40, 40, 50))
        ailette = t.masque(lambda d, x=x, y=y: d.polygon([(x - 3, y - 6), (x + 3, y - 6), (x + 1, y - 2), (x - 1, y - 2)], fill=1))
        t.objet(ailette, FER, (170, 170, 180), (40, 40, 50))
        t.point(x, y, (255, 60, 40))
    t.vignette(0.4)
    return t


def etheriumBastion():
    """Ultime : la forteresse s'élève, l'ingénieur sur le rempart, le canon de siège tonne en cyan"""
    t = Toile()
    t.fond_radial(24, 20, 42, [(100, 50, 170), (50, 16, 100), (20, 6, 44), (6, 2, 14)])
    fissure(t, 2, 45, 46, 44)
    mur(t, 4, 26, 44, 43, pierre=(90, 70, 110), joint=CYAN)
    for x in range(4, 44, 8):
        creneau = t.masque(lambda d, x=x: d.rectangle([x, 22, x + 4, 26], fill=1))
        t.objet(creneau, (90, 70, 110), (140, 120, 170), (40, 30, 60))
    heros(t, "brasTendu", 12, 20, s=0.75, couleur=(20, 10, 30), liseres=((CYAN, 0, -1),))
    corps = t.masque(lambda d: d.rounded_rectangle([26, 14, 38, 24], 2, fill=1))
    t.objet(corps, CUIVRE, CUIVRE_C, (90, 50, 20))
    tube = t.masque(lambda d: d.line([(32, 18), (44, 10)], fill=1, width=4))
    t.objet(tube, FER, (170, 170, 180), (40, 40, 50))
    t.point(28, 16, CYAN)
    explosion(t, 44, 8, 4, couleurs=(BLANC, CYAN, (120, 80, 220)), branches=6)
    for x, y in ((8, 6), (22, 4)):
        explosion(t, x, y, 3, couleurs=(BLANC, CYAN, (120, 80, 220)), branches=5)
    t.vignette(0.3)
    return t


def flamme(t, x, sol, h, w, penche=0, graine=1):
    """Langue de feu ondulée : base large, pointe recourbée, coeur clair"""
    import random
    rnd = random.Random(graine)
    pts_g, pts_d = [], []
    for i in range(6):
        f = i / 5
        dx = penche * f * f + math.sin(f * 3.2 + graine) * 1.2
        lw = w * (1 - f) ** 0.8 * (0.85 + rnd.uniform(0, 0.3))
        y = sol - h * f
        pts_g.append((x + dx - lw, y)); pts_d.append((x + dx + lw, y))
    pts = pts_g + [(x + penche + 0.5, sol - h - 1)] + pts_d[::-1]
    m = t.masque(lambda d: d.polygon(pts, fill=1))
    t.objet(m, (255, 150, 30), (255, 230, 140), (220, 60, 20), contour=(100, 20, 6))
    coeur = [(px * 0.5 + x * 0.5 + penche * 0.3, sol - (sol - py) * 0.6) for px, py in pts]
    t.peindre(t.masque(lambda d: d.polygon(coeur, fill=1)), (255, 240, 170))
