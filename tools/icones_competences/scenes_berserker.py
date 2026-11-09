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
    """Bordeaux : la main entaillée, l'arme rougeoie de sang bouillant"""
    t = Toile()
    t.fond_radial(24, 20, 42, [(240, 120, 110), (150, 30, 40), (70, 6, 16), (20, 0, 4)])
    t.lueur(28, 16, 14, RAGE, 0.6)
    hache(t, 14, 44, 32, 12, metal=(255, 120, 90))
    main = t.masque(lambda d: (d.ellipse([4, 28, 16, 38], fill=1), d.rectangle([4, 34, 12, 46], fill=1)))
    t.objet(main, (220, 160, 130), (250, 200, 170), (140, 90, 70))
    t.peindre(t.masque(lambda d: d.line([(6, 32), (14, 30)], fill=1)), SANG)
    sang(t, ((10, 40, 1.5), (14, 44, 1)))
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
    """Cramoisi : une rafale de coups chaotiques, le sang gicle partout"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(255, 160, 140), (210, 40, 50), (110, 6, 20), (36, 0, 6)])
    for a, b in ((200, 300), (20, 120), (250, 350), (100, 190)):
        croissant(t, 24, 24, 15 + (a % 3) * 2, a, b, (255, 220, 210), epais=2)
    sang(t, ((10, 12, 2), (38, 10, 2.5), (8, 34, 1.5), (40, 36, 2), (24, 6, 1.5), (24, 42, 2)))
    t.vignette(0.4)
    return t


def gapingWound():
    """Rouge-brun : le poing frappe le sol en sang, une onde rouge repousse tout"""
    t = Toile()
    t.fond_vertical([(60, 10, 10), (150, 40, 30), (200, 90, 60)], 0, 36)
    sol_m = t.masque(lambda d: d.rectangle([0, 36, 48, 48], fill=1))
    t.zone_verticale(sol_m, [(90, 30, 20), (30, 8, 4)], 36, 48)
    for r in (10, 17, 23):
        anneau(t, 24, 39, r, r * 0.3, RAGE if r < 20 else (200, 40, 30), 1)
    poing = t.masque(lambda d: (d.rounded_rectangle([14, 24, 34, 37], 4, fill=1), d.polygon([(17, 4), (31, 4), (30, 26), (18, 26)], fill=1)))
    t.objet(poing, (210, 150, 120), (250, 200, 170), (130, 80, 60))
    for x in (19, 24, 29):
        t.peindre(t.masque(lambda d, x=x: d.line([(x, 28), (x, 36)], fill=1)), (130, 80, 60))
    sang(t, ((14, 34, 2), (34, 33, 2), (24, 40, 2.5)))
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
    """Brun sombre : la main agrippe la gorge, puis l'arme achève"""
    t = Toile()
    t.fond_radial(28, 22, 42, [(230, 170, 130), (160, 80, 50), (80, 30, 20), (24, 8, 4)])
    heros(t, "tombe", 34, 26, s=0.8, couleur=(40, 16, 10), liseres=((OR_PALE, 0, -1),), retourne=True)
    main = t.masque(lambda d: (d.line([(4, 30), (24, 22)], fill=1, width=4), d.ellipse([22, 18, 30, 26], fill=1)))
    t.objet(main, (200, 140, 110), (240, 190, 160), (120, 70, 50))
    hache(t, 10, 12, 26, 4)
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
    """Rouge tourbillon : une tornade rouge avance en taillant et saigne elle-même"""
    t = Toile()
    t.fond_radial(24, 26, 40, [(250, 170, 150), (200, 60, 50), (110, 16, 20), (36, 4, 6)])
    tornade(t, 24, 42, 6, 16, (200, 20, 30), clair=(255, 180, 160), n=8)
    hache(t, 30, 30, 42, 18, double=True)
    sang(t, ((8, 20, 1.5), (40, 40, 2), (10, 38, 1.5)))
    t.vignette(0.4)
    return t


def carnalShockwave():
    """Pourpre : il se frappe le torse, le choc et le cri paralysent la zone"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(240, 150, 190), (170, 40, 90), (90, 10, 40), (30, 2, 12)])
    for r in (14, 19, 23):
        anneau(t, 24, 22, r, r, (255, 200, 220), 1)
    heros(t, "garde", 24, 26, s=1.0, liseres=((RAGE, 0, -1),))
    explosion(t, 26, 18, 3, couleurs=(BLANC, (255, 200, 200), RAGE), branches=6)
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
