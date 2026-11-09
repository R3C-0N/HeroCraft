"""Scènes du Sorcier élémentaire : chaque élément porte sa couleur, les fusions les mêlent."""
import math

from pieces import *  # noqa: F401,F403
from scenes import Toile, dilater

EAU = (90, 180, 255)
EAU_C = (200, 240, 255)
PIERRE = (140, 120, 100)
VAPEUR = (240, 245, 250)


def gantelet(t, cx, cy, couleur=(120, 110, 130), lueur=None):
    """Poing ganté levé"""
    if lueur:
        t.lueur(cx, cy - 4, 9, lueur, 0.7)
    m = t.masque(lambda d: (d.rounded_rectangle([cx - 5, cy - 5, cx + 5, cy + 4], 2, fill=1),
                            d.rectangle([cx - 4, cy + 4, cx + 4, cy + 12], fill=1)))
    t.objet(m, couleur, melange(couleur, BLANC, 0.4), melange(couleur, (0, 0, 0), 0.5))
    for i in range(3):
        t.peindre(t.masque(lambda d, i=i: d.line([(cx - 3 + i * 3, cy - 5), (cx - 3 + i * 3, cy - 2)], fill=1)),
                  melange(couleur, (0, 0, 0), 0.5))


def rocher(t, cx, cy, r, couleur=PIERRE, givre=None):
    pts = [(cx + r * math.cos(math.radians(a)) * (0.8 + 0.2 * (i % 2)), cy + r * math.sin(math.radians(a)) * (0.8 + 0.2 * ((i + 1) % 2)))
           for i, a in enumerate(range(0, 360, 45))]
    m = t.masque(lambda d: d.polygon(pts, fill=1))
    t.objet(m, couleur, melange(couleur, BLANC, 0.4), melange(couleur, (0, 0, 0), 0.5))
    if givre:
        t.liseré(m, givre, -1, -1)
    return m


def igneousOrb():
    """Feu : une boule de feu file hors de la main"""
    t = Toile()
    t.fond_radial(30, 18, 44, [(255, 220, 140), (240, 110, 40), (160, 40, 20), (70, 14, 10), (24, 4, 4)])
    boule_de_feu(t, 30, 18, 7, vers=(-0.7, 0.7))
    gantelet(t, 10, 36, lueur=(255, 140, 40))
    t.vignette(0.4)
    return t


def volcanicRune():
    """Feu : une rune rougeoie au sol et jaillit en pilier de feu"""
    t = Toile()
    t.fond_vertical([(40, 10, 10), (120, 30, 20), (200, 70, 30)], 0, 36)
    sol = t.masque(lambda d: d.rectangle([0, 36, 48, 48], fill=1))
    t.zone_verticale(sol, [(60, 30, 24), (20, 10, 8)], 36, 48)
    pilier = t.masque(lambda d: d.polygon([(18, 40), (16, 20), (20, 8), (24, 2), (28, 10), (32, 20), (30, 40)], fill=1))
    t.objet(pilier, (255, 150, 40), (255, 240, 160), (220, 60, 20), contour=(90, 20, 10))
    t.peindre(t.masque(lambda d: d.polygon([(21, 38), (22, 18), (24, 10), (26, 18), (27, 38)], fill=1)), (255, 240, 170))
    anneau(t, 24, 40, 18, 4, (255, 90, 40), 2, clair=(255, 200, 100))
    t.lueur(24, 30, 18, (255, 120, 40), 0.5)
    t.vignette(0.4)
    return t


def foamingWhip():
    """Eau : un fouet d'eau claque en arc, des gouttes bleues restent sur la cible"""
    t = Toile()
    t.fond_radial(24, 24, 42, [(200, 240, 255), (70, 160, 230), (20, 80, 160), (8, 30, 70)])
    t.peindre(t.masque(lambda d: d.arc([6, 6, 44, 40], 160, 350, fill=1, width=4)), EAU)
    t.peindre(t.masque(lambda d: d.arc([6, 6, 44, 40], 200, 340, fill=1, width=1)), BLANC)
    for x, y in ((40, 26), (43, 32), (38, 34), (44, 20)):
        orbe(t, x, y, 1.5, EAU_C, halo=False)
    gantelet(t, 10, 34, lueur=EAU)
    t.vignette(0.4)
    return t


def waterVeil():
    """Eau : une bulle d'eau ondule autour du sorcier, quelques bulles en montent"""
    t = Toile()
    t.fond_radial(24, 26, 40, [(120, 220, 240), (40, 140, 200), (14, 70, 130), (4, 24, 50)])
    bulle = t.masque(lambda d: d.ellipse([8, 6, 40, 44], fill=1))
    t.voile(bulle, EAU, 0.35)
    heros(t, "garde", 24, 26, s=0.9, liseres=((EAU_C, 0, -1),))
    for i in range(4):
        r = 16 + (i % 2)
        t.peindre(t.masque(lambda d, i=i, r=r: d.arc([24 - r, 25 - r - 2, 24 + r, 25 + r + 2], i * 90, i * 90 + 60, fill=1, width=2)), EAU_C)
    for x, y in ((14, 8), (32, 4), (38, 12)):
        anneau(t, x, y, 1.5, 1.5, BLANC, 1)
    t.vignette(0.4)
    return t


def seismicShard():
    """Terre : un rocher lancé éclate à l'impact"""
    t = Toile()
    t.fond_radial(30, 30, 44, [(230, 200, 150), (170, 120, 70), (100, 60, 30), (40, 22, 10)])
    explosion(t, 34, 32, 9, couleurs=((240, 220, 180), (190, 150, 100), (120, 80, 50)), branches=7)
    for x, y in ((40, 24), (44, 36), (28, 40)):
        rocher(t, x, y, 2)
    rocher(t, 22, 22, 7)
    for i in range(3):
        t.peindre(t.masque(lambda d, i=i: d.line([(4 + i * 3, 8 + i * 4), (14 + i * 2, 16 + i * 2)], fill=1)), (240, 220, 180))
    t.vignette(0.4)
    return t


def telluricRampart():
    """Terre : un mur de pierre surgit et avance en labourant le sol"""
    t = Toile()
    t.fond_vertical([(110, 140, 170), (170, 180, 170), (200, 180, 140)], 0, 34)
    sol = t.masque(lambda d: d.rectangle([0, 36, 48, 48], fill=1))
    t.zone_verticale(sol, [(130, 100, 60), (60, 40, 24)], 36, 48)
    mur(t, 12, 12, 36, 38, pierre=(150, 130, 110))
    for x, y in ((6, 34), (42, 32), (8, 26), (40, 22)):
        rocher(t, x, y, 2, couleur=(130, 100, 70))
    nuage(t, 24, 40, 40, 6, (180, 150, 110), graine=5)
    t.vignette(0.4)
    return t


def galeBlades():
    """Air : des lames d'air pâles partent en éventail"""
    t = Toile()
    t.fond_radial(14, 30, 46, [(250, 255, 255), (180, 220, 235), (100, 150, 190), (40, 70, 110), (14, 26, 44)])
    for a in (-40, -15, 10):
        croissant(t, 12, 34, 30, a - 12, a + 12, (220, 245, 255), epais=3)
    gantelet(t, 10, 36, couleur=(140, 140, 160), lueur=(220, 240, 255))
    t.vignette(0.4)
    return t


def updraft():
    """Air : le sorcier est soufflé en arrière, une rafale part devant lui"""
    t = Toile()
    t.fond_radial(28, 20, 44, [(240, 250, 255), (160, 210, 230), (80, 140, 180), (30, 60, 90), (10, 20, 34)])
    heros(t, "saut", 14, 22, s=0.9, liseres=((BLANC, 0, -1),), retourne=True)
    for y in (16, 24, 32):
        t.peindre(t.masque(lambda d, y=y: d.arc([22, y - 5, 46, y + 5], 270, 360, fill=1, width=2)), BLANC)
    nuage(t, 14, 42, 24, 6, (210, 200, 180), graine=3)
    t.vignette(0.4)
    return t


def fusion():
    """Les quatre éléments tournent autour d'un même cœur"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(240, 240, 255), (150, 140, 200), (70, 60, 120), (20, 16, 40)])
    for (x, y), c in zip(((24, 9), (39, 24), (24, 39), (9, 24)), ((240, 90, 30), EAU, (150, 110, 60), (220, 245, 255))):
        orbe(t, x, y, 4, c)
    anneau(t, 24, 24, 15, 15, (220, 220, 255), 1)
    orbe(t, 24, 24, 4, BLANC)
    t.vignette(0.4)
    return t


def scaldingTyphoon():
    """Feu et eau : le fouet frappe l'orbe en vol, une énorme vague de vapeur éclate"""
    t = Toile()
    t.fond_radial(28, 20, 44, [(250, 250, 250), (200, 210, 220), (130, 120, 140), (70, 50, 60), (24, 16, 20)])
    nuage(t, 28, 18, 34, 22, VAPEUR, sombre=(180, 190, 200), graine=6, n=8)
    boule_de_feu(t, 28, 20, 4)
    t.peindre(t.masque(lambda d: d.arc([2, 14, 34, 50], 200, 330, fill=1, width=3)), EAU)
    t.vignette(0.4)
    return t


def shrapnelStorm():
    """Terre et air : le rocher éclate en plein vol, des éclats pleuvent sur la zone"""
    t = Toile()
    t.fond_vertical([(160, 190, 210), (200, 190, 160), (160, 120, 80)], 0, 48)
    explosion(t, 24, 12, 7, couleurs=((250, 240, 220), (210, 200, 180), (160, 140, 110)), branches=8)
    for x, y in ((10, 22), (18, 30), (30, 26), (38, 34), (14, 40), (26, 40), (40, 20)):
        rocher(t, x, y, 2)
        t.peindre(t.masque(lambda d, x=x, y=y: d.line([(x - 2, y - 5), (x, y - 2)], fill=1)), BLANC)
    t.vignette(0.4)
    return t


def magmaWall():
    """Feu et terre : un mur de pierre incandescent laisse une flaque de lave"""
    t = Toile()
    t.fond_vertical([(30, 10, 10), (90, 24, 16), (160, 50, 20)], 0, 36)
    lave = t.masque(lambda d: d.ellipse([2, 36, 46, 46], fill=1))
    t.objet(lave, (255, 120, 30), (255, 220, 100), (200, 50, 20), contour=(60, 10, 6))
    mur(t, 12, 10, 36, 38, pierre=(70, 40, 34), joint=(255, 120, 40), rougeoie=(255, 100, 30))
    for x, y in ((10, 41), (30, 43), (38, 40)):
        anneau(t, x, y, 1.5, 1, (255, 240, 150), 1)
    t.vignette(0.4)
    return t


def soothingMist():
    """Eau et air : un brouillard blanc-bleu enveloppe la zone, des bulles de soin autour du sorcier"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(240, 250, 255), (180, 220, 240), (110, 160, 200), (40, 70, 110)])
    heros(t, "brasOuverts", 24, 26, s=0.85, couleur=(40, 60, 90), liseres=((BLANC, 0, -1),))
    nuage(t, 24, 38, 48, 16, (230, 240, 250), graine=4, alpha=0.75)
    nuage(t, 24, 10, 48, 10, (230, 240, 250), graine=8, alpha=0.6)
    for x, y in ((12, 22), (36, 20), (16, 32), (34, 32)):
        anneau(t, x, y, 2, 2, (120, 255, 200), 1)
    t.vignette(0.35)
    return t


def explosiveRetreat():
    """Feu et air : le sorcier bondit en arrière, l'orbe explose là où il était"""
    t = Toile()
    t.fond_radial(32, 32, 44, [(255, 230, 150), (240, 120, 40), (150, 40, 30), (60, 14, 16), (20, 4, 6)])
    explosion(t, 32, 34, 12)
    heros(t, "saut", 12, 18, s=0.8, liseres=(((255, 220, 150), 1, 0),), retourne=True)
    t.peindre(t.masque(lambda d: d.arc([10, 14, 40, 50], 200, 300, fill=1, width=1)), (255, 240, 200))
    t.vignette(0.4)
    return t


def crystallization():
    """Eau et terre : le rocher se couvre de givre et éclate en éclats glacés"""
    t = Toile()
    t.fond_radial(28, 24, 42, [(240, 250, 255), (160, 220, 250), (70, 130, 200), (20, 50, 100), (6, 16, 40)])
    rocher(t, 18, 22, 8, couleur=(150, 180, 210), givre=BLANC)
    cristaux(t, ((32, 20, 8), (38, 30, 7), (30, 34, 6), (40, 14, 5)), couleur=(200, 240, 255), sombre=(80, 140, 220))
    for x, y in ((8, 10), (44, 40), (24, 40)):
        t.etincelle(x, y, (220, 245, 255))
    t.vignette(0.4)
    return t


def voidSingularity():
    """Ultime : les gantelets aspirent la lumière vers un point noir qui grossit"""
    t = Toile()
    t.fond_radial(24, 22, 40, [(160, 110, 230), (90, 40, 170), (40, 12, 80), (10, 2, 20)])
    for i in range(10):
        a = math.radians(i * 36 + 10)
        t.peindre(t.masque(lambda d, a=a: d.line([(24 + 20 * math.cos(a), 22 + 20 * math.sin(a)),
                                                   (24 + 9 * math.cos(a + 0.4), 22 + 9 * math.sin(a + 0.4))], fill=1)), (200, 170, 255))
    anneau(t, 24, 22, 9, 9, CYAN, 2)
    t.peindre(t.masque(lambda d: d.ellipse([17, 15, 31, 29], fill=1)), (4, 0, 8))
    gantelet(t, 8, 38, couleur=(90, 80, 110), lueur=VIOLET)
    gantelet(t, 40, 38, couleur=(90, 80, 110), lueur=VIOLET)
    t.vignette(0.3)
    return t
