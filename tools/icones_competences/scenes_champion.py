"""Scènes du Champion : le bouclier d'abord, la ligne qui ne recule pas."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater

CHAMP = (160, 40, 40)


def shieldBash():
    """Orange sec : un coup de bouclier, des étoiles tournent sur la cible"""
    t = Toile()
    t.fond_radial(28, 22, 44, [(255, 220, 160), (230, 140, 60), (140, 60, 20), (50, 20, 6)])
    heros(t, "tombe", 36, 30, s=0.7, couleur=(30, 14, 8), liseres=((OR_PALE, 0, -1),))
    bouclier(t, 18, 22, 10, champ=CHAMP)
    for x, y in ((32, 12), (40, 10), (44, 18)):
        t.etincelle(x, y, (255, 240, 80), grande=True)
    t.vignette(0.4)
    return t


def stoppingLunge():
    """Vert mousse : un estoc bas aux jambes"""
    t = Toile()
    t.fond_radial(28, 26, 44, [(220, 240, 190), (120, 170, 100), (50, 90, 50), (14, 30, 16)])
    heros(t, "course", 36, 22, s=0.8, couleur=(20, 30, 20), liseres=((BLANC, 0, -1),), retourne=True)
    lame(t, 4, 36, 32, 34)
    t.vignette(0.4)
    return t


def taunt():
    """Rouge colère : un cri, le monstre visé porte une marque rouge"""
    t = Toile()
    t.fond_radial(20, 24, 44, [(250, 180, 150), (200, 70, 50), (110, 20, 20), (40, 4, 6)])
    heros(t, "brasOuverts", 12, 28, s=0.8, liseres=((OR_PALE, 0, -1),))
    ondes(t, 16, 14, (255, 220, 200), rayons=(6, 11), debut=-30, fin=30)
    bete(t, "ours", 34, 32, s=0.65, couleur=(30, 10, 10), liseres=((BLANC, 0, -1),), retourne=True)
    t.peindre(t.masque(lambda d: d.polygon([(30, 12), (36, 12), (33, 18)], fill=1)), (255, 40, 30))
    t.vignette(0.4)
    return t


def ironGuard():
    """Fer sombre : bouclier levé, chaque coup bloqué fait jaillir des étincelles"""
    t = Toile()
    t.fond_radial(24, 22, 40, [(210, 210, 220), (120, 120, 140), (50, 50, 70), (14, 14, 22)])
    bouclier(t, 24, 20, 12, champ=CHAMP)
    for x, y in ((8, 12), (40, 10), (6, 28), (42, 30), (24, 4)):
        t.etincelle(x, y, (255, 220, 120))
    t.vignette(0.4)
    return t


def phalanx():
    """Poussière qui retombe : le champion s'ancre, un cercle d'acier à ses pieds"""
    t = Toile()
    t.fond_radial(24, 30, 42, [(220, 210, 190), (150, 140, 120), (70, 64, 56), (20, 18, 16)])
    anneau(t, 24, 40, 18, 4, (220, 225, 235), 2, clair=BLANC)
    nuage(t, 8, 40, 12, 5, (200, 190, 170), graine=2)
    nuage(t, 40, 40, 12, 5, (200, 190, 170), graine=3)
    heros(t, "garde", 22, 28, liseres=((BLANC, 0, -1),))
    bouclier(t, 30, 24, 7, champ=CHAMP)
    t.vignette(0.4)
    return t


def armorBreaker():
    """Acier sous la forge : un coup lourd fait voler des éclats d'armure"""
    t = Toile()
    t.fond_radial(28, 28, 44, [(255, 210, 160), (210, 110, 60), (120, 40, 20), (40, 10, 4)])
    casque = t.masque(lambda d: (d.chord([22, 20, 42, 42], 180, 360, fill=1), d.rectangle([22, 30, 42, 38], fill=1)))
    t.objet(casque, (170, 175, 190), BLANC, (80, 80, 100))
    t.peindre(t.masque(lambda d: d.line([(26, 32), (38, 32)], fill=1, width=2)), (20, 14, 20))
    for x, y in ((18, 16), (44, 18), (16, 30), (42, 10)):
        t.peindre(t.masque(lambda d, x=x, y=y: d.polygon([(x, y), (x + 3, y - 1), (x + 2, y + 2)], fill=1)), (200, 205, 220))
    lame(t, 6, 6, 30, 22, largeur=3)
    t.vignette(0.4)
    return t


def rallyingCry():
    """Or de bannière : un cri plus ample, tous les ennemis de la zone sont marqués"""
    t = Toile()
    t.fond_radial(24, 24, 44, [(255, 240, 190), (230, 170, 70), (150, 80, 30), (60, 26, 10)])
    ondes(t, 24, 14, (255, 250, 220), rayons=(8, 14, 20), debut=-60, fin=240)
    heros(t, "brasLeves", 24, 30, s=0.8, liseres=((BLANC, 0, -1),))
    for x in (6, 42):
        t.peindre(t.masque(lambda d, x=x: d.polygon([(x - 3, 30), (x + 3, 30), (x, 36)], fill=1)), (255, 40, 30))
    t.vignette(0.4)
    return t


def perfectRiposte():
    """Bleu roi : une parade dans un éclat, suivie d'un coup d'épée"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(200, 220, 255), (80, 110, 200), (24, 40, 110), (6, 12, 40)])
    bouclier(t, 16, 22, 9, champ=CHAMP)
    explosion(t, 24, 16, 5, couleurs=(BLANC, (255, 240, 180), (255, 200, 90)), branches=8)
    croissant(t, 26, 26, 16, -60, 60, (230, 240, 255), epais=3)
    lame(t, 26, 40, 42, 22)
    t.vignette(0.4)
    return t


def armoredBreach():
    """Brun de bataille : charge bouclier en avant, puis l'empalement"""
    t = Toile()
    t.fond_vertical([(130, 120, 110), (200, 160, 110), (170, 120, 70)], 0, 38)
    sol_m = t.masque(lambda d: d.rectangle([0, 38, 48, 48], fill=1))
    t.zone_verticale(sol_m, [(110, 80, 50), (50, 34, 20)], 38, 48)
    heros(t, "course", 18, 26, liseres=((OR_PALE, 1, 0),))
    bouclier(t, 28, 20, 7, champ=CHAMP)
    lame(t, 26, 30, 46, 30)
    t.vignette(0.4)
    return t


def livingWall():
    """Gris-bleu : une ligne de front translucide avance devant le champion"""
    t = Toile()
    t.fond_radial(24, 26, 44, [(210, 220, 235), (120, 140, 170), (50, 64, 90), (14, 18, 30)])
    front = t.masque(lambda d: d.rectangle([28, 6, 36, 42], fill=1))
    t.voile(front, (200, 220, 255), 0.5)
    t.peindre(t.masque(lambda d: d.line([(36, 6), (36, 42)], fill=1, width=2)), BLANC)
    heros(t, "garde", 16, 28, liseres=((BLANC, 1, 0),))
    bouclier(t, 24, 24, 6, champ=CHAMP)
    for y in (12, 24, 36):
        t.peindre(t.masque(lambda d, y=y: d.line([(40, y), (45, y)], fill=1)), BLANC)
    t.vignette(0.4)
    return t


def devastatingCounter():
    """Rouge sombre : il encaisse sur le bouclier, puis fauche la zone"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(250, 180, 150), (190, 60, 50), (100, 16, 20), (30, 4, 6)])
    t.peindre(t.masque(lambda d: d.ellipse([2, 20, 46, 36], outline=1, width=3)), (255, 230, 210))
    heros(t, "garde", 24, 24, s=0.85, liseres=((BLANC, 0, -1),))
    bouclier(t, 31, 20, 6, champ=CHAMP)
    t.vignette(0.4)
    return t


def voidMonolith():
    """Ultime : le sol s'effondre en cratère, le champion y brille, invulnérable"""
    t = Toile()
    t.fond_radial(24, 26, 42, [(110, 60, 180), (54, 20, 100), (20, 6, 44), (6, 2, 14)])
    cratere = t.masque(lambda d: d.ellipse([2, 34, 46, 48], fill=1))
    t.peindre(cratere, (14, 4, 24))
    anneau(t, 24, 41, 22, 7, CYAN, 1)
    t.lueur(24, 26, 16, (200, 150, 255), 0.6)
    heros(t, "garde", 24, 28, couleur=(255, 236, 200), liseres=((BLANC, 0, -1), ((200, 140, 255), 0, 1)))
    bouclier(t, 31, 24, 6, champ=(90, 40, 160), embleme=CYAN)
    cristaux(t, ((6, 36, 6), (42, 36, 6)))
    t.vignette(0.3)
    return t
