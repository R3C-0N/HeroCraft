"""Scènes du Prêtre-guerrier : un prêtre au maillet, entre lumière sacrée et coups lourds.
Chaque compétence a sa propre ambiance de couleur (indiquée en tête de chaque scène)."""
import math

from scenes import N, Toile, dessiner_maillet, dilater, silhouette

OR = (255, 206, 72)
OR_PALE = (255, 240, 170)
BLANC = (255, 255, 255)
OMBRE = (14, 8, 20)


def righteousStrike():
    """Coucher de soleil orange : le maillet s'abat dans une traînée dorée, des étincelles au
    contact, une lueur de soin remonte sur le prêtre"""
    t = Toile()
    t.fond_radial(34, 34, 46, [(255, 236, 140), (250, 150, 40), (200, 60, 30), (90, 20, 30), (30, 8, 20)])
    # traînée de l'élan
    trainee = t.masque(lambda d: d.arc([4, 4, 52, 52], 190, 300, fill=1, width=4))
    t.voile(trainee, OR_PALE, 0.7)
    t.lueur(33, 33, 16, (255, 220, 120), 0.9)
    t.rayons(33, 33, 7, 15, 10, OR_PALE, decalage=8)
    dessiner_maillet(t, 9, 42, 30, 29, tete=(12, 7))
    for x, y in ((40, 22), (43, 31), (37, 41), (25, 38)):
        t.etincelle(x, y, OR)
    for x, y in ((8, 18), (12, 12), (5, 26)):
        t.etincelle(x, y, (140, 255, 140))
    t.vignette(0.4)
    return t


def healingBolt():
    """Vert d'eau : une main ouverte lance un trait de lumière qui éclate en paillettes de soin"""
    t = Toile()
    t.fond_radial(32, 16, 50, [(200, 255, 220), (60, 200, 170), (20, 110, 110), (10, 50, 60), (6, 20, 30)])
    # trait de lumière
    trait = t.masque(lambda d: d.line([(14, 34), (36, 14)], fill=1, width=3))
    t.lueur(25, 24, 14, (180, 255, 200), 0.6)
    t.peindre(trait, (230, 255, 200))
    t.peindre(t.masque(lambda d: d.line([(15, 33), (35, 15)], fill=1, width=1)), BLANC)
    t.lueur(36, 14, 9, (255, 255, 200), 1.0)
    for x, y in ((40, 8), (43, 17), (31, 7), (38, 22)):
        t.etincelle(x, y, (120, 255, 150))
    # main levée, paume vers la lumière
    main = t.masque(lambda d: (d.ellipse([5, 34, 17, 46], fill=1),
                               [d.line([(6 + i * 3, 37), (8 + i * 3.6, 29 - (i in (1, 2)) * 2)], fill=1, width=2)
                                for i in range(4)],
                               d.line([(15, 42), (20, 36)], fill=1, width=2)))
    t.objet(main, (232, 180, 130), (255, 220, 180), (150, 96, 70), lumiere=(1, -1))
    t.lueur(12, 32, 6, OR_PALE, 0.6)
    t.vignette(0.45)
    return t


def judgment():
    """Ciel d'ambre lourd, sol fendu : le maillet frappe et une onde dorée court au sol"""
    t = Toile()
    t.fond_vertical([(60, 26, 20), (150, 70, 30), (230, 150, 60)], 0, 32)
    sol = t.masque(lambda d: d.rectangle([0, 32, N, N], fill=1))
    t.zone_verticale(sol, [(110, 70, 40), (60, 36, 24), (24, 14, 12)], 32, N)
    for r in (8, 15, 22):
        onde = t.masque(lambda d, r=r: d.ellipse([24 - r * 1.1, 37 - r * 0.32, 24 + r * 1.1, 37 + r * 0.32], outline=1, width=1))
        t.peindre(onde & sol, OR if r < 20 else (220, 150, 60))
    fissures = t.masque(lambda d: [d.line(l, fill=1) for l in
                                   (((20, 38), (12, 42), (6, 41)), ((28, 38), (34, 43), (41, 44)), ((24, 39), (23, 46)))])
    t.peindre(fissures, (255, 230, 120))
    t.lueur(24, 34, 13, (255, 210, 110), 0.9, sy=0.6)
    for x, y in ((12, 28), (36, 27), (17, 22), (31, 21)):
        t.point(x, y, (180, 120, 70)); t.point(x + 1, y + 1, (120, 80, 50))
    dessiner_maillet(t, 24, 6, 24, 30, tete=(14, 8))
    t.vignette(0.45)
    return t


def faithArmor():
    """Nuit bleue : le prêtre agenouillé dans un dôme doré translucide"""
    t = Toile()
    t.fond_radial(24, 30, 40, [(70, 90, 190), (34, 44, 120), (16, 20, 60), (6, 8, 24)])
    for x, y in ((6, 6), (40, 9), (32, 4), (12, 14), (44, 18)):
        t.point(x, y, (200, 210, 255))
    dome = t.masque(lambda d: d.pieslice([5, 10, 43, 56], 180, 360, fill=1))
    t.voile(dome, (255, 210, 90), 0.25)
    t.lueur(24, 34, 16, (255, 200, 90), 0.5)
    t.peindre(t.masque(lambda d: d.arc([5, 10, 43, 56], 180, 360, fill=1, width=1)), OR)
    t.peindre(t.masque(lambda d: d.arc([9, 14, 39, 52], 200, 250, fill=1, width=1)), OR_PALE)
    t.peindre(t.masque(lambda d: d.rectangle([3, 33, 45, 34], fill=1)), (255, 190, 70))
    # prêtre agenouillé, mains jointes
    corps = t.masque(lambda d: silhouette(d, {
        "tete": (24, 16), "cou": (24, 20), "hanche": (24, 27),
        "coudeG": (20, 24), "mainG": (23, 21), "coudeD": (28, 24), "mainD": (25, 21),
        "genouG": (19, 29), "piedG": (16, 32), "genouD": (28, 31), "piedD": (26, 32),
        "cape": [(30, 32), (18, 32)]}))
    t.peindre(corps, OMBRE)
    t.liseré(corps, (255, 220, 120), 0, -1)
    t.vignette(0.5)
    return t


def hallowedHammer():
    """Lumière sacrée blanche et or : le maillet consacré, auréolé, rayonne"""
    t = Toile()
    t.fond_radial(24, 18, 40, [(255, 255, 235), (255, 230, 140), (240, 160, 50), (170, 80, 30), (70, 24, 20)])
    t.rayons(24, 16, 10, 30, 8, (255, 250, 210), decalage=22)
    aureole = t.masque(lambda d: d.ellipse([15, 3, 33, 9], outline=1, width=2))
    t.objet(aureole, (255, 220, 90), BLANC, (200, 130, 40), contour=(120, 60, 20))
    dessiner_maillet(t, 24, 44, 24, 16, tete=(16, 9), metal=(240, 236, 220), lumiere=(-1, 0))
    for x, y in ((10, 30), (38, 32)):
        t.etincelle(x, y, (255, 255, 200), coeur=(255, 255, 255))
    t.vignette(0.35)
    return t


def devotionAura():
    """Violet profond : le prêtre debout dans un cercle doré qui monte de ses pieds"""
    t = Toile()
    t.fond_radial(24, 38, 44, [(170, 90, 200), (100, 40, 150), (50, 16, 90), (20, 6, 40)])
    colonne = t.masque(lambda d: d.rectangle([9, 0, 39, 40], fill=1))
    t.lueur(24, 40, 18, (255, 190, 90), 0.7, sy=2.2)
    for x, y in ((10, 24), (38, 20)):
        t.etincelle(x, y, (255, 230, 150))
    anneau = t.masque(lambda d: d.ellipse([5, 34, 43, 45], outline=1, width=2))
    t.objet(anneau, OR, OR_PALE, (190, 110, 30), contour=(60, 20, 50))
    corps = t.masque(lambda d: silhouette(d, {
        "tete": (24, 12), "cou": (24, 17), "hanche": (24, 28),
        "coudeG": (18, 21), "mainG": (14, 25), "coudeD": (30, 21), "mainD": (34, 25),
        "genouG": (21, 34), "piedG": (19, 39), "genouD": (27, 34), "piedD": (29, 39)}))
    t.peindre(corps, OMBRE)
    t.liseré(corps, (255, 210, 120), 0, 1)
    t.liseré(corps, (200, 120, 230), -1, 0)
    t.vignette(0.45)
    return t


def penitentCharge():
    """Rouge sang, lignes de vitesse : le prêtre fonce, maillet en avant, la cible voit des étoiles"""
    t = Toile()
    t.fond_radial(38, 22, 46, [(255, 170, 110), (220, 50, 40), (130, 16, 30), (50, 6, 16)])
    for y, x0, x1 in ((8, 2, 18), (30, 0, 10)):
        t.peindre(t.masque(lambda d, y=y, x0=x0, x1=x1: d.line([(x0, y), (x1, y)], fill=1)), (255, 140, 110))
    poussiere = t.masque(lambda d: [d.ellipse(b, fill=1) for b in ([2, 36, 12, 45], [8, 38, 18, 46], [0, 40, 8, 47])])
    t.objet(poussiere, (190, 120, 90), (240, 180, 140), (110, 60, 50), contour=(70, 20, 25))
    corps = t.masque(lambda d: silhouette(d, {
        "tete": (25, 13), "cou": (23, 18), "hanche": (18, 28),
        "coudeG": (26, 23), "mainG": (31, 22), "coudeD": (28, 20), "mainD": (32, 19),
        "genouG": (24, 33), "piedG": (22, 40), "genouD": (12, 33), "piedD": (8, 37),
        "cape": [(10, 30), (8, 22)]}))
    t.peindre(corps, OMBRE)
    t.liseré(corps, (255, 160, 120), 1, 0)
    dessiner_maillet(t, 28, 22, 38, 17, tete=(9, 5))
    t.lueur(41, 17, 7, (255, 230, 150), 0.9)
    for x, y in ((37, 8), (44, 10), (45, 25)):
        t.etincelle(x, y, (255, 240, 80), grande=True)
    t.vignette(0.4)
    return t


def sanctuary():
    """Bleu-vert de crypte : un sceau doré tourne au sol, la lumière en monte"""
    t = Toile()
    t.fond_radial(24, 34, 44, [(120, 220, 210), (30, 130, 140), (12, 60, 80), (4, 22, 34)])
    t.lueur(24, 30, 20, (255, 230, 150), 0.55, sy=2.0)
    for r, w in ((20, 2), (14, 1)):
        m = t.masque(lambda d, r=r, w=w: d.ellipse([24 - r, 37 - r * 0.33, 24 + r, 37 + r * 0.33], outline=1, width=w))
        t.peindre(m, OR if w == 2 else OR_PALE)
    etoile = t.masque(lambda d: d.polygon([(24 + 17 * math.cos(math.radians(a)), 37 + 5.6 * math.sin(math.radians(a)))
                                           for a in (270, 54, 198, 342, 126)], outline=1))
    t.peindre(etoile, (255, 230, 140))
    for x, y in ((14, 22), (32, 14), (24, 6)):
        t.etincelle(x, y, (255, 240, 170))
    t.vignette(0.5)
    return t


def purifyingWave():
    """Azur : un anneau de lumière s'élargit depuis le prêtre et lave tout sur son passage"""
    t = Toile()
    t.fond_radial(24, 24, 40, [(230, 250, 255), (120, 200, 250), (40, 110, 200), (16, 40, 110), (8, 16, 50)])
    for r, c in ((21, (200, 240, 255)), (15, BLANC), (9, (255, 245, 200))):
        anneau = t.masque(lambda d, r=r: d.ellipse([24 - r, 26 - r, 24 + r, 26 + r], outline=1, width=2))
        t.peindre(anneau, c)
    t.lueur(24, 26, 9, (255, 240, 180), 0.8)
    corps = t.masque(lambda d: silhouette(d, {
        "tete": (24, 16), "cou": (24, 20), "hanche": (24, 29),
        "coudeG": (19, 23), "mainG": (15, 21), "coudeD": (29, 23), "mainD": (33, 21),
        "genouG": (21, 34), "piedG": (19, 39), "genouD": (27, 34), "piedD": (29, 39)}))
    t.peindre(corps, (20, 30, 70))
    t.liseré(corps, BLANC, 0, -1)
    for x, y in ((6, 10), (42, 12), (8, 40), (41, 40)):
        t.etincelle(x, y, (180, 240, 255))
    t.vignette(0.45)
    return t


def bastionOfLight():
    """Crépuscule magenta : un grand dôme doré posé sur la plaine, que rien ne traverse"""
    t = Toile()
    t.fond_vertical([(70, 20, 80), (200, 60, 120), (255, 150, 90)], 0, 34)
    sol = t.masque(lambda d: d.rectangle([0, 34, N, N], fill=1))
    t.zone_verticale(sol, [(80, 40, 50), (40, 20, 34), (16, 8, 18)], 34, N)
    dome = t.masque(lambda d: d.pieslice([2, 6, 46, 62], 180, 360, fill=1))
    t.voile(dome, (255, 220, 120), 0.35)
    t.peindre(t.masque(lambda d: d.arc([2, 6, 46, 62], 180, 360, fill=1, width=2)), OR)
    t.peindre(t.masque(lambda d: d.arc([6, 10, 42, 58], 195, 240, fill=1, width=1)), BLANC)
    for x in (10, 24, 38):
        t.peindre(t.masque(lambda d, x=x: d.line([(x, 34), (24 + (x - 24) * 0.4, 8)], fill=1)), (255, 230, 150))
    onde = t.masque(lambda d: d.ellipse([0, 32, 48, 40], outline=1, width=1))
    t.peindre(onde & sol, (255, 200, 100))
    t.lueur(24, 34, 14, (255, 220, 140), 0.6, sy=0.5)
    t.vignette(0.4)
    return t


def condemnation():
    """Noir et rouge : la cible tombe, une colonne de lumière blanche la frappe d'en haut"""
    t = Toile()
    t.fond_radial(24, 36, 40, [(160, 30, 30), (90, 10, 20), (40, 4, 12), (12, 2, 6)])
    t.lueur(24, 24, 22, (255, 200, 160), 0.35, sy=3)
    colonne = t.masque(lambda d: d.polygon([(19, 0), (29, 0), (31, 40), (17, 40)], fill=1))
    t.voile(colonne, (255, 240, 220), 0.55)
    t.peindre(t.masque(lambda d: d.rectangle([22, 0, 26, 40], fill=1)), BLANC)
    cible = t.masque(lambda d: (d.ellipse([16, 30, 32, 46], fill=1), d.ellipse([12, 38, 36, 48], fill=1)))
    t.peindre(cible, OMBRE)
    t.liseré(cible, (255, 200, 180), 0, -1)
    crane = t.masque(lambda d: (d.ellipse([20, 32, 28, 40], fill=1)))
    t.peindre(crane, (60, 20, 30))
    t.point(22, 36, (255, 80, 60)); t.point(25, 36, (255, 80, 60))
    t.lueur(24, 40, 12, (255, 240, 220), 0.7, sy=0.4)
    for x, y in ((10, 30), (38, 26)):
        t.point(x, y, (255, 120, 60))
    t.vignette(0.5)
    return t


def avatarOfJustice():
    """Ultime : noir d'Étherium, le prêtre devient une immense silhouette de lumière"""
    t = Toile()
    t.fond_radial(24, 22, 40, [(110, 50, 170), (60, 20, 110), (24, 8, 50), (6, 2, 14)])
    t.lueur(24, 22, 22, (255, 200, 110), 0.7)
    t.rayons(24, 20, 16, 30, 6, (170, 110, 230), decalage=30)
    corps = t.masque(lambda d: silhouette(d, {
        "tete": (24, 9), "cou": (24, 14), "hanche": (24, 28),
        "coudeG": (16, 12), "mainG": (10, 5), "coudeD": (32, 12), "mainD": (38, 5),
        "genouG": (20, 36), "piedG": (16, 45), "genouD": (28, 36), "piedD": (32, 45),
        "cape": [(33, 40), (15, 40)]}))
    corps = dilater(corps, 1)
    t.peindre(corps, (255, 236, 160))
    t.liseré(corps, BLANC, 0, -1)
    t.liseré(corps, (230, 150, 60), 0, 1)
    t.contour(corps, (90, 30, 120))
    for x, y in ((7, 28), (41, 28), (8, 42), (40, 42)):
        t.etincelle(x, y, (110, 245, 255))
    t.vignette(0.3)
    return t
