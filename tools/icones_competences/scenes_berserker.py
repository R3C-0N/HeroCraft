"""Scènes du Berserker : la hache, le sang et la rage, en rouge mais pas toujours le même."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater

RAGE = (255, 60, 40)


def savageStrike():
    """Rouge brique : un coup désordonné et une gerbe de sang"""
    t = Toile()
    t.fond_radial(28, 24, 42, [(250, 170, 130), (200, 70, 40), (110, 20, 16), (36, 4, 4)])
    croissant(t, 24, 24, 18, 210, 320, (255, 220, 200), epais=3)
    hache(t, 10, 42, 30, 14)
    sang(t, ((36, 26, 3), (42, 18, 2), (40, 34, 2), (32, 36, 1.5), (44, 28, 1.5)))
    t.vignette(0.4)
    return t


def bloodyCut():
    """Bordeaux : le berserker s'entaille le bras, sa hache rougeoie de sang bouillant"""
    t = Toile()
    t.fond_radial(24, 20, 42, [(240, 120, 110), (150, 30, 40), (70, 6, 16), (20, 0, 4)])
    sol = t.masque(lambda d: d.rectangle([0, 42, 48, 48], fill=1))
    t.peindre(sol, (40, 4, 10))
    t.lueur(32, 14, 12, RAGE, 0.7)
    corps, q = heros(t, "brasLeves", 22, 30, s=0.95, liseres=((RAGE, 0, -1),))
    mx, my = q["mainD"]
    hache(t, mx - 3, my + 4, mx + 3, my - 8, metal=(255, 120, 90))
    gx, gy = q["mainG"]
    t.peindre(t.masque(lambda d: d.line([(gx + 1, gy + 3), (gx + 4, gy + 6)], fill=1, width=2)), (255, 40, 40))
    sang(t, ((gx + 3, gy + 10, 1.5), (gx + 4, gy + 16, 1.5), (gx + 2, 40, 2)))
    t.vignette(0.4)
    return t


def visceralHowl():
    """Rouge de gorge : un cri rouge fait vaciller les ennemis"""
    t = Toile()
    t.fond_radial(16, 22, 44, [(255, 170, 140), (220, 60, 40), (120, 14, 16), (40, 2, 4)])
    heros(t, "brasOuverts", 12, 28, s=0.85, liseres=((RAGE, 0, -1),))
    ondes(t, 18, 13, RAGE, rayons=(7, 13, 19, 25), debut=-35, fin=35)
    t.vignette(0.4)
    return t


def blindCharge():
    """Rouille : une ruée tête baissée qui renverse la cible"""
    t = Toile()
    t.fond_vertical([(150, 120, 100), (210, 130, 80), (170, 80, 40)], 0, 38)
    sol_m = t.masque(lambda d: d.rectangle([0, 38, 48, 48], fill=1))
    t.zone_verticale(sol_m, [(110, 60, 30), (40, 20, 10)], 38, 48)
    nuage(t, 6, 38, 14, 8, (220, 170, 120), graine=3)
    heros(t, "course", 20, 26, s=1.0, liseres=((RAGE, 1, 0),))
    heros(t, "tombe", 40, 30, s=0.6, couleur=(40, 20, 10), liseres=((OR_PALE, 0, -1),))
    t.vignette(0.4)
    return t


def frenziedMutilation():
    """Cramoisi : le berserker taille dans tous les sens, le sang gicle partout"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(255, 160, 140), (210, 40, 50), (110, 6, 20), (36, 0, 6)])
    for a, b in ((200, 300), (20, 120), (250, 350), (100, 190)):
        croissant(t, 24, 24, 17 + (a % 3) * 2, a, b, (255, 220, 210), epais=2)
    corps, q = heros(t, "frappeHaute", 22, 30, s=0.9, liseres=((RAGE, 0, -1),))
    mx, my = q["mainD"]
    hache(t, mx - 2, my + 4, mx + 6, my - 6)
    sang(t, ((8, 12, 2), (40, 12, 2.5), (8, 34, 1.5), (40, 34, 2), (34, 6, 1.5)))
    t.vignette(0.4)
    return t


def gapingWound():
    """Rouge-brun : la hache s'abat dans le sol ensanglanté, une onde rouge repousse tout"""
    t = Toile()
    t.fond_vertical([(60, 10, 10), (150, 40, 30), (200, 90, 60)], 0, 36)
    sol_m = t.masque(lambda d: d.rectangle([0, 36, 48, 48], fill=1))
    t.zone_verticale(sol_m, [(90, 30, 20), (30, 8, 4)], 36, 48)
    for r in (12, 18, 23):
        anneau(t, 24, 40, r, r * 0.3, RAGE if r < 20 else (200, 40, 30), 1)
    corps, q = heros(t, "agenouille", 20, 30, s=1.0, liseres=((RAGE, 0, -1),))
    mx, my = q["mainD"]
    hache(t, mx - 2, my, mx + 6, 40)
    explosion(t, mx + 6, 40, 4, couleurs=(BLANC, (255, 160, 140), RAGE), branches=6)
    sang(t, ((10, 34, 1.5), (38, 33, 2), (34, 26, 1.5)))
    t.vignette(0.4)
    return t


def dyingFrenzy():
    """Noir et rouge battant : un halo rouge s'intensifie à mesure que la vie baisse"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(120, 20, 20), (60, 6, 10), (24, 2, 4), (6, 0, 0)])
    t.lueur(24, 24, 22, RAGE, 0.8)
    heros(t, "agenouille", 24, 32, s=1.1, couleur=(20, 4, 6), liseres=((RAGE, 0, -1), ((255, 180, 150), 1, 0)))
    for i, w in enumerate((10, 6, 3)):
        t.peindre(t.masque(lambda d, i=i, w=w: d.rectangle([6 + i * 13, 44, 6 + i * 13 + w, 46], fill=1)), RAGE)
    t.vignette(0.3)
    return t


def crushingGrip():
    """Brun sombre : une main soulève l'ennemi par la gorge, l'autre lève la hache"""
    t = Toile()
    t.fond_radial(28, 22, 42, [(230, 170, 130), (160, 80, 50), (80, 30, 20), (24, 8, 4)])
    sol = t.masque(lambda d: d.rectangle([0, 42, 48, 48], fill=1))
    t.peindre(sol, (40, 16, 8))
    pendu = dict(POSES["debout"])
    pendu.update({"piedG": (-2, 12), "piedD": (2, 12), "genouG": (-2, 6), "genouD": (2, 6),
                  "mainG": (-3, 1), "mainD": (3, 1), "coudeG": (-4, -5), "coudeD": (4, -5)})
    heros(t, pendu, 33, 30, s=0.85, couleur=(60, 30, 20), liseres=((OR_PALE, 0, -1),), retourne=True)
    q = dict(POSES["brasTendu"])
    q.update({"mainD": (12, -14), "coudeD": (6, -11), "coudeG": (-5, -12), "mainG": (-3, -18)})
    corps, q = heros(t, q, 20, 31, s=0.9, liseres=((RAGE, 1, 0),))
    gx, gy = q["mainG"]
    hache(t, gx, gy + 3, gx - 2, gy - 10)
    t.vignette(0.4)
    return t


def hemoglobinEruption():
    """Écarlate brûlant : le sang sur l'arme explose en une bulle rouge"""
    t = Toile()
    t.fond_radial(28, 24, 42, [(255, 210, 170), (240, 90, 50), (150, 20, 20), (50, 4, 6)])
    bulle = t.masque(lambda d: d.ellipse([16, 8, 44, 36], fill=1))
    t.voile(bulle, (255, 40, 30), 0.45)
    anneau(t, 30, 22, 14, 14, (255, 200, 170), 1)
    explosion(t, 30, 22, 7, couleurs=((255, 230, 200), (255, 100, 60), (200, 20, 20)), branches=8)
    hache(t, 6, 44, 24, 26)
    t.vignette(0.4)
    return t


def agonyWhirl():
    """Rouge tourbillon : le berserker tournoie hache tendue au cœur d'une tornade de sang"""
    t = Toile()
    t.fond_radial(24, 26, 40, [(250, 170, 150), (200, 60, 50), (110, 16, 20), (36, 4, 6)])
    sol = t.masque(lambda d: d.rectangle([0, 42, 48, 48], fill=1))
    t.peindre(sol, (50, 8, 10))
    for i, (y, r) in enumerate(((14, 14), (24, 19), (34, 16))):
        t.peindre(t.masque(lambda d, y=y, r=r, i=i: d.arc([24 - r, y - 4, 24 + r, y + 4], i * 90 + 160, i * 90 + 400, fill=1, width=2)),
                  (255, 120, 110))
    corps, q = heros(t, "brasOuverts", 24, 30, s=0.95, liseres=((RAGE, 0, -1),))
    mx, my = q["mainD"]
    hache(t, mx - 2, my + 1, mx + 9, my - 2)
    sang(t, ((6, 20, 1.5), (42, 38, 2), (8, 38, 1.5)))
    t.vignette(0.4)
    return t


def carnalShockwave():
    """Pourpre : il se frappe le torse, le choc et le cri paralysent la zone"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(240, 150, 190), (170, 40, 90), (90, 10, 40), (30, 2, 12)])
    sol = t.masque(lambda d: d.rectangle([0, 42, 48, 48], fill=1))
    t.peindre(sol, (40, 6, 20))
    for r in (13, 18, 22):
        anneau(t, 24, 22, r, r, (255, 200, 220), 1)
    q = dict(POSES["debout"])
    q.update({"coudeD": (6, -5), "mainD": (1, -8), "coudeG": (-6, -12), "mainG": (-10, -18)})
    corps, q = heros(t, q, 24, 30, s=1.0, liseres=((RAGE, 0, -1),))
    gx, gy = q["mainG"]
    hache(t, gx + 2, gy + 3, gx - 1, gy - 9)
    for a in (-30, 0, 30):
        r = math.radians(a)
        t.peindre(t.masque(lambda d, r=r: d.line([(31 + 2 * math.cos(r), 22 + 2 * math.sin(r)), (31 + 6 * math.cos(r), 22 + 6 * math.sin(r))],
                                                  fill=1)), (255, 220, 230))
    t.vignette(0.4)
    return t


def etheriumAbomination():
    """Ultime : il s'enfonce l'éclat dans la chair et devient colossal"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(120, 50, 170), (60, 16, 100), (24, 4, 44), (6, 0, 14)])
    t.lueur(24, 26, 20, (200, 60, 120), 0.5)
    heros(t, "brasOuverts", 24, 30, s=1.35, couleur=(110, 30, 70), liseres=((CYAN, 0, -1), ((255, 60, 80), 0, 1)), epais=1)
    cristaux(t, ((24, 22, 7), (16, 18, 4), (32, 18, 4)))
    t.vignette(0.3)
    return t
