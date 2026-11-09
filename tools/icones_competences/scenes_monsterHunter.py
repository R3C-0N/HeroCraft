"""Scènes du Chasseur de monstre : lame courte, arbalète et harpon, le sang des bêtes."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater

SANG_C = (240, 60, 60)


def monstre(t, x, y, s=1.0, liseres=((SANG_C, 0, -1),), retourne=False):
    """Une grosse bête cornue, vue de profil"""
    m = bete(t, "ours", x, y, s=s, couleur=(26, 14, 16), liseres=liseres, retourne=retourne)
    k = -1 if retourne else 1
    cornes = t.masque(lambda d: d.line([(x + k * 9 * s, y - 8 * s), (x + k * 7 * s, y - 14 * s), (x + k * 3 * s, y - 16 * s)],
                                       fill=1, width=2))
    t.peindre(cornes, (230, 220, 190))
    t.point(x + k * 14 * s, y - 4 * s, (255, 200, 40))
    return m


def quickSlash():
    """Rouge vif : une taille rapide qui fait jaillir le sang"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(255, 190, 170), (210, 60, 60), (120, 16, 30), (40, 4, 10)])
    croissant(t, 24, 24, 18, 200, 330, (255, 240, 230), epais=3)
    lame(t, 8, 40, 26, 20)
    sang(t, ((34, 12, 2.5), (40, 18, 2), (38, 8, 1.5), (44, 26, 1.5)))
    t.vignette(0.4)
    return t


def hinderingShot():
    """Vert de forêt : un carreau aux jambes, la bête ralentit"""
    t = Toile()
    t.fond_radial(28, 26, 44, [(200, 230, 170), (100, 150, 90), (40, 70, 40), (12, 26, 14)])
    monstre(t, 32, 26, s=0.85, liseres=((BLANC, 0, -1),), retourne=True)
    fleche(t, 4, 34, 22, 34, couleur=(200, 200, 210), epais=2)
    nuage(t, 30, 36, 14, 4, (200, 220, 255), graine=3, alpha=0.6)
    t.vignette(0.4)
    return t


def tacticalDodge():
    """Brun poussiéreux : une roulade de côté"""
    t = Toile()
    t.fond_vertical([(170, 160, 140), (200, 170, 120), (160, 120, 80)], 0, 38)
    sol = t.masque(lambda d: d.rectangle([0, 40, 48, 48], fill=1))
    t.zone_verticale(sol, [(120, 90, 60), (50, 36, 24)], 40, 48)
    heros(t, "accroupi", 30, 30, liseres=((SANG_C, 0, -1),))
    t.peindre(t.masque(lambda d: d.arc([4, 18, 34, 48], 180, 300, fill=1, width=2)), BLANC)
    nuage(t, 10, 40, 16, 6, (210, 190, 150), graine=4)
    t.vignette(0.4)
    return t


def pommelStrike():
    """Or terne : un coup sec du pommeau, des étoiles tournent sur la cible"""
    t = Toile()
    t.fond_radial(28, 20, 42, [(250, 230, 170), (200, 150, 80), (110, 70, 30), (40, 20, 8)])
    monstre(t, 30, 30, s=0.9, liseres=((OR_PALE, 0, -1),), retourne=True)
    lame(t, 4, 30, 18, 22, garde=True)
    explosion(t, 20, 20, 4, couleurs=(BLANC, (255, 240, 180), (240, 200, 100)), branches=6)
    for x, y in ((24, 10), (32, 6), (40, 10)):
        t.etincelle(x, y, (255, 240, 80), grande=True)
    t.vignette(0.4)
    return t


def piercingVolley():
    """Bleu nuit : trois carreaux d'affilée en éventail"""
    t = Toile()
    t.fond_radial(24, 26, 44, [(170, 190, 230), (70, 90, 150), (24, 30, 70), (6, 8, 24)])
    arbalete(t, 2, 26, couleur=(80, 50, 40))
    for a in (-18, 0, 18):
        r = math.radians(a)
        fleche(t, 18 + 2 * math.cos(r), 26 + 2 * math.sin(r), 18 + 26 * math.cos(r), 26 + 26 * math.sin(r), couleur=(220, 225, 240),
               trainee=(140, 160, 220))
    t.vignette(0.4)
    return t


def monsterSkewer():
    """Pourpre sombre : un estoc profond, un trait rouge"""
    t = Toile()
    t.fond_radial(28, 24, 44, [(230, 140, 160), (150, 40, 70), (70, 10, 30), (20, 2, 8)])
    monstre(t, 32, 28, s=0.9, liseres=((SANG_C, 0, -1),), retourne=True)
    t.peindre(t.masque(lambda d: d.line([(4, 24), (40, 24)], fill=1, width=3)), (240, 40, 50))
    lame(t, 4, 24, 36, 24, largeur=2)
    sang(t, ((40, 18, 1.5), (42, 30, 2)))
    t.vignette(0.4)
    return t


def harpoonBolt():
    """Gris d'acier : un câble tendu entre le chasseur et la bête, qui se rétracte"""
    t = Toile()
    t.fond_radial(24, 24, 44, [(220, 220, 225), (140, 140, 150), (70, 70, 80), (20, 20, 26)])
    monstre(t, 34, 30, s=0.85, liseres=((BLANC, 0, -1),), retourne=True)
    t.peindre(t.masque(lambda d: d.line([(10, 28), (30, 26)], fill=1)), (230, 220, 200))
    arbalete(t, 0, 28, couleur=(80, 50, 40))
    for x in (14, 20, 26):
        t.peindre(t.masque(lambda d, x=x: d.line([(x + 2, 24), (x, 27), (x + 2, 30)], fill=1)), BLANC)
    t.vignette(0.4)
    return t


def crossRiposte():
    """Acier et étincelles : la lame pare, puis un tir à bout portant"""
    t = Toile()
    t.fond_radial(24, 22, 42, [(250, 230, 200), (180, 140, 100), (90, 60, 40), (30, 18, 10)])
    lame(t, 8, 40, 34, 10)
    lame(t, 40, 40, 14, 12, metal=(170, 170, 180))
    explosion(t, 24, 22, 5, couleurs=(BLANC, (255, 230, 150), (255, 170, 60)), branches=8)
    for x, y in ((16, 16), (32, 14), (30, 28)):
        t.etincelle(x, y, (255, 220, 120))
    t.vignette(0.4)
    return t


def deadlyDance():
    """Rouge et cuivre : le chasseur avance en taillant, ses carreaux partent sur les côtés"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(255, 200, 160), (210, 90, 60), (120, 30, 30), (40, 8, 8)])
    croissant(t, 24, 26, 16, 160, 380, (255, 230, 210), epais=2)
    heros(t, "brasOuverts", 24, 28, s=0.8, liseres=((OR_PALE, 0, -1),))
    for c in (-1, 1):
        fleche(t, 24 + c * 8, 16, 24 + c * 22, 10, couleur=(220, 220, 230))
    t.vignette(0.4)
    return t


def deathblow():
    """Noir et rouge : la bête tombe, un empalement vers le haut l'achève"""
    t = Toile()
    t.fond_radial(24, 30, 42, [(200, 60, 60), (120, 16, 26), (50, 4, 12), (14, 0, 4)])
    monstre(t, 24, 32, s=1.0, liseres=((SANG_C, 0, -1),))
    lame(t, 18, 46, 28, 8, largeur=2)
    sang(t, ((24, 18, 2), (30, 14, 1.5), (20, 12, 1.5)))
    t.vignette(0.45)
    return t


def lightningAssault():
    """Orage jaune : le chasseur se projette sur la bête pour une frappe plongeante"""
    t = Toile()
    t.fond_radial(28, 30, 44, [(255, 250, 200), (230, 200, 80), (130, 100, 40), (40, 30, 10)])
    monstre(t, 32, 34, s=0.8, liseres=((BLANC, 0, -1),), retourne=True)
    heros(t, "frappeHaute", 18, 16, s=0.75, liseres=((BLANC, 1, 0),))
    lame(t, 20, 4, 30, 22, largeur=2)
    t.peindre(t.masque(lambda d: d.line([(4, 4), (16, 14)], fill=1, width=1)), BLANC)
    t.vignette(0.4)
    return t


def etheriumFury():
    """Ultime : la lame et les carreaux brillent d'Étherium"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(110, 50, 180), (54, 16, 100), (20, 4, 44), (6, 0, 14)])
    lame(t, 8, 40, 30, 14, metal=(200, 230, 255), lueur=CYAN)
    fleche(t, 8, 10, 46, 10, couleur=CYAN, trainee=(150, 90, 230), fut=(140, 100, 200))
    anneau(t, 30, 14, 9, 9, (190, 140, 255), 1)
    cristaux(t, ((38, 28, 5), (42, 38, 4)))
    t.vignette(0.3)
    return t
