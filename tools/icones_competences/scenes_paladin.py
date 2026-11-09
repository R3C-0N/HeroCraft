"""Scènes du Paladin : épée et bouclier, la lumière comme arme."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater


def radiantStrike():
    """Blanc aveuglant : l'épée frappe, un flash éblouit la cible"""
    t = Toile()
    t.fond_radial(32, 30, 42, [(255, 255, 240), (255, 230, 130), (230, 150, 40), (130, 60, 20), (40, 14, 10)])
    t.rayons(32, 30, 6, 22, 12, BLANC, decalage=10)
    explosion(t, 32, 30, 8, couleurs=(BLANC, (255, 250, 200), (255, 220, 120)), branches=8)
    croissant(t, 24, 26, 18, 200, 300, (255, 240, 170), epais=3)
    lame(t, 10, 40, 30, 20, lueur=(255, 240, 180))
    t.vignette(0.35)
    return t


def shieldFlash():
    """Bleu-vert : le bouclier lâche un flash et une petite onde"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(200, 255, 240), (70, 200, 190), (20, 110, 120), (8, 50, 60), (4, 20, 26)])
    t.rayons(24, 22, 12, 24, 10, (230, 255, 250), decalage=0)
    anneau(t, 24, 22, 19, 19, (230, 255, 250), 1)
    t.lueur(24, 22, 14, BLANC, 0.6)
    bouclier(t, 24, 20, 10, champ=(70, 120, 220))
    t.vignette(0.4)
    return t


def sealOfBravery():
    """Rouge royal : un sceau doré s'allume sous le paladin et ses alliés"""
    t = Toile()
    t.fond_radial(24, 30, 44, [(250, 120, 100), (190, 30, 40), (110, 10, 30), (40, 4, 14)])
    t.lueur(24, 38, 20, (255, 210, 120), 0.6, sy=0.5)
    anneau(t, 24, 39, 21, 6, OR, 2, clair=BLANC)
    anneau(t, 24, 39, 14, 4, OR_PALE, 1)
    for a in range(0, 360, 60):
        t.point(24 + 17.5 * math.cos(math.radians(a)), 39 + 5 * math.sin(math.radians(a)), BLANC)
    heros(t, "debout", 10, 30, s=0.8, couleur=(60, 20, 30), liseres=((OR_PALE, 0, -1),))
    heros(t, "debout", 38, 30, s=0.8, couleur=(60, 20, 30), liseres=((OR_PALE, 0, -1),))
    corps, q = heros(t, "brasLeves", 24, 28, s=0.9, liseres=((OR_PALE, 0, -1),))
    mx, my = q["mainD"]
    lame(t, mx, my + 2, mx + 3, my - 8, garde=True)
    t.vignette(0.35)
    return t


def wingedAssault():
    """Bleu ciel : la charge bouclier en avant, dans une traînée de plumes lumineuses"""
    t = Toile()
    t.fond_radial(34, 20, 44, [(240, 250, 255), (140, 200, 250), (60, 120, 210), (20, 50, 120), (8, 20, 50)])
    for x, y, a in ((4, 14, 10), (8, 26, -10), (3, 34, 0), (12, 8, 20), (14, 36, -20)):
        plume(t, x, y, a, couleur=(255, 245, 210))
    heros(t, "course", 24, 26, liseres=((BLANC, 1, 0),))
    bouclier(t, 35, 20, 7)
    t.vignette(0.4)
    return t


def glimmeringGuard():
    """Bleu nuit : un dôme doré absorbe la magie, des paillettes bleues en montent"""
    t = Toile()
    t.fond_radial(24, 30, 40, [(60, 80, 180), (30, 40, 120), (14, 18, 60), (4, 6, 24)])
    dome = t.masque(lambda d: d.pieslice([6, 10, 42, 62], 180, 360, fill=1))
    t.voile(dome, (255, 220, 110), 0.25)
    t.peindre(t.masque(lambda d: d.arc([6, 10, 42, 62], 180, 360, fill=1, width=2)), OR)
    heros(t, "garde", 24, 25, s=0.8, liseres=((OR_PALE, 0, -1),))
    bouclier(t, 30, 21, 5)
    for x, y in ((10, 30), (14, 18), (36, 14), (40, 28), (24, 6)):
        t.etincelle(x, y, (120, 200, 255))
    t.vignette(0.45)
    return t


def dawnWeapon():
    """Aube orange et rose : l'épée dressée s'embrase de lumière"""
    t = Toile()
    t.fond_vertical([(90, 40, 120), (230, 90, 110), (255, 170, 80), (255, 220, 140)], 0, 48)
    t.lueur(24, 22, 18, (255, 240, 180), 0.9)
    t.rayons(24, 20, 10, 24, 8, (255, 250, 220), decalage=0)
    lame(t, 24, 46, 24, 6, largeur=3, lueur=(255, 255, 220))
    for x, y in ((18, 12), (30, 16), (20, 26), (29, 30)):
        t.etincelle(x, y, (255, 250, 200))
    t.vignette(0.35)
    return t


def vigorAura():
    """Émeraude : un cercle lumineux persistant autour du paladin"""
    t = Toile()
    t.fond_radial(24, 30, 42, [(170, 250, 170), (60, 180, 100), (20, 100, 60), (8, 40, 24)])
    anneau(t, 24, 38, 18, 5, (255, 240, 150), 2, clair=BLANC)
    t.lueur(24, 30, 18, (220, 255, 170), 0.5, sy=1.8)
    heros(t, "debout", 24, 27, liseres=((BLANC, 0, -1),))
    bouclier(t, 31, 24, 5)
    for x, y in ((8, 24), (40, 22), (12, 14), (36, 10)):
        t.etincelle(x, y, (220, 255, 190))
    t.vignette(0.4)
    return t


def virtuousIntervention():
    """Lavande : le paladin atterrit dans un éclat de lumière devant l'allié"""
    t = Toile()
    t.fond_radial(28, 34, 44, [(250, 230, 255), (190, 160, 240), (120, 90, 190), (50, 34, 100), (16, 10, 36)])
    explosion(t, 30, 40, 10, couleurs=(BLANC, (255, 240, 180), (230, 200, 255)), branches=10)
    t.peindre(t.masque(lambda d: d.arc([6, 0, 52, 70], 200, 260, fill=1, width=2)), BLANC)
    heros(t, "agenouille", 11, 31, s=0.8, couleur=(80, 56, 120), liseres=((BLANC, 0, -1),))
    heros(t, "accroupi", 30, 31, liseres=((BLANC, 0, -1),))
    bouclier(t, 39, 27, 5)
    t.vignette(0.35)
    return t


def solarWave():
    """Soleil de midi : l'épée frappe le bouclier, une onde dorée part tout autour"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(255, 255, 210), (255, 220, 80), (250, 140, 30), (160, 60, 20), (60, 20, 10)])
    for r in (20, 15, 10):
        anneau(t, 24, 24, r, r, BLANC if r == 15 else (255, 240, 170), 2)
    bouclier(t, 24, 22, 8)
    lame(t, 36, 36, 26, 18, garde=True)
    explosion(t, 27, 20, 4, couleurs=(BLANC, (255, 250, 200), (255, 220, 120)), branches=6)
    t.vignette(0.35)
    return t


def celestialPhalanx():
    """Bleu et or : un mur de lumière avance devant le paladin et bouscule les ennemis"""
    t = Toile()
    t.fond_vertical([(30, 40, 100), (70, 100, 180), (150, 170, 220)], 0, 38)
    sol = t.masque(lambda d: d.rectangle([0, 38, 48, 48], fill=1))
    t.zone_verticale(sol, [(70, 70, 90), (24, 24, 36)], 38, 48)
    heros(t, "garde", 11, 28, s=0.9, liseres=((OR_PALE, 1, 0),))
    bouclier(t, 19, 23, 5)
    mur_m = t.masque(lambda d: d.polygon([(25, 6), (30, 4), (30, 42), (25, 40)], fill=1))
    t.voile(mur_m, (255, 230, 140), 0.55)
    t.peindre(t.masque(lambda d: d.line([(30, 4), (30, 42)], fill=1, width=2)), OR)
    t.lueur(28, 24, 12, OR_PALE, 0.4, sy=2)
    heros(t, "course", 39, 28, s=0.75, couleur=(40, 30, 60), liseres=((OR_PALE, -1, 0),), retourne=True)
    for y in (12, 22, 32):
        t.peindre(t.masque(lambda d, y=y: d.line([(32, y), (35, y)], fill=1)), (255, 240, 190))
    t.vignette(0.35)
    return t


def wrathOfTheJust():
    """Cramoisi et or : deux coups croisés, chaque impact lâche une étincelle de soin"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(255, 200, 120), (220, 70, 40), (140, 20, 30), (60, 6, 16), (20, 2, 6)])
    croissant(t, 24, 24, 17, 200, 320, (255, 230, 150), epais=3)
    croissant(t, 24, 24, 17, 20, 140, (255, 230, 150), epais=3)
    lame(t, 12, 38, 34, 12, lueur=(255, 220, 140))
    for x, y in ((24, 24), (10, 14), (38, 34)):
        t.etincelle(x, y, (150, 255, 150), grande=(x == 24))
    t.vignette(0.4)
    return t


def voidFortress():
    """Ultime : un dôme astral ; le paladin au centre lance des lances de lumière cyan"""
    t = Toile()
    t.fond_radial(24, 28, 42, [(100, 50, 170), (50, 20, 100), (20, 6, 44), (6, 2, 14)])
    for x, y in ((6, 6), (14, 3), (38, 5), (44, 14), (30, 2)):
        t.point(x, y, (220, 200, 255))
    dome = t.masque(lambda d: d.pieslice([4, 8, 44, 66], 180, 360, fill=1))
    t.voile(dome, (150, 90, 240), 0.35)
    t.peindre(t.masque(lambda d: d.arc([4, 8, 44, 66], 180, 360, fill=1, width=2)), (180, 120, 255))
    heros(t, "garde", 24, 28, s=0.85, liseres=((CYAN, 0, -1),))
    bouclier(t, 30, 24, 5, champ=(90, 40, 160), embleme=CYAN)
    for x0, y0, x1, y1 in ((20, 20, 6, 10), (28, 18, 42, 8), (24, 16, 24, 2)):
        t.peindre(t.masque(lambda d, a=(x0, y0, x1, y1): d.line([(a[0], a[1]), (a[2], a[3])], fill=1, width=2)), CYAN)
        t.etincelle(x1, y1, CYAN)
    t.vignette(0.35)
    return t
