"""Pièces réutilisables pour les scènes : poses de héros, armes, projectiles, créatures,
nuages, éclairs… Chaque fonction dessine directement sur une `Toile`."""
import math

from scenes import N, dilater, melange, silhouette

OMBRE = (14, 8, 20)
BLANC = (255, 255, 255)
ACIER = (206, 214, 228)
BOIS = (150, 92, 44)
OR = (255, 206, 72)
OR_PALE = (255, 240, 170)
CYAN = (110, 245, 255)
VIOLET = (150, 70, 230)
SANG = (200, 20, 30)

# --- Poses : articulations autour de la hanche (0, 0), tête vers -16, pieds vers +11 ----------

POSES = {
    "debout": {"tete": (0, -15), "cou": (0, -10), "hanche": (0, 0), "coudeG": (-5, -6), "mainG": (-6, 0),
               "coudeD": (5, -6), "mainD": (6, 0), "genouG": (-3, 6), "piedG": (-4, 11), "genouD": (3, 6), "piedD": (4, 11)},
    "brasLeves": {"tete": (0, -15), "cou": (0, -10), "hanche": (0, 0), "coudeG": (-6, -13), "mainG": (-10, -19),
                  "coudeD": (6, -13), "mainD": (10, -19), "genouG": (-3, 6), "piedG": (-5, 11), "genouD": (3, 6), "piedD": (5, 11)},
    "brasOuverts": {"tete": (0, -15), "cou": (0, -10), "hanche": (0, 0), "coudeG": (-6, -8), "mainG": (-11, -10),
                    "coudeD": (6, -8), "mainD": (11, -10), "genouG": (-3, 6), "piedG": (-5, 11), "genouD": (3, 6), "piedD": (5, 11)},
    "brasTendu": {"tete": (0, -15), "cou": (0, -10), "hanche": (0, 0), "coudeG": (-4, -5), "mainG": (-3, 0),
                  "coudeD": (6, -10), "mainD": (12, -11), "genouG": (-3, 6), "piedG": (-5, 11), "genouD": (3, 6), "piedD": (5, 11)},
    "course": {"tete": (4, -15), "cou": (2, -10), "hanche": (-1, 0), "coudeG": (-4, -5), "mainG": (-7, -2),
               "coudeD": (6, -7), "mainD": (10, -10), "genouG": (5, 4), "piedG": (4, 11), "genouD": (-4, 6), "piedD": (-10, 9)},
    "saut": {"tete": (1, -15), "cou": (0, -10), "hanche": (-1, 0), "coudeG": (-6, -12), "mainG": (-8, -17),
             "coudeD": (6, -9), "mainD": (10, -13), "genouG": (-6, 2), "piedG": (-4, 8), "genouD": (4, 3), "piedD": (1, 9)},
    "accroupi": {"tete": (2, -10), "cou": (1, -6), "hanche": (-1, 2), "coudeG": (-4, -2), "mainG": (-1, 3),
                 "coudeD": (6, -3), "mainD": (10, -4), "genouG": (5, 3), "piedG": (4, 9), "genouD": (-5, 6), "piedD": (-8, 9)},
    "agenouille": {"tete": (1, -12), "cou": (0, -7), "hanche": (-1, 2), "coudeG": (3, -3), "mainG": (5, -7),
                   "coudeD": (4, -2), "mainD": (6, -6), "genouG": (5, 3), "piedG": (5, 10), "genouD": (-1, 10), "piedD": (-8, 10)},
    "garde": {"tete": (0, -15), "cou": (0, -10), "hanche": (0, 0), "coudeG": (4, -6), "mainG": (8, -7),
              "coudeD": (5, -4), "mainD": (8, -2), "genouG": (-4, 6), "piedG": (-6, 11), "genouD": (4, 6), "piedD": (6, 11)},
    "frappeHaute": {"tete": (1, -15), "cou": (0, -10), "hanche": (0, 0), "coudeG": (-2, -15), "mainG": (2, -19),
                    "coudeD": (3, -15), "mainD": (4, -19), "genouG": (-4, 6), "piedG": (-6, 11), "genouD": (4, 6), "piedD": (6, 11)},
    "tir": {"tete": (1, -15), "cou": (0, -10), "hanche": (0, 0), "coudeG": (6, -10), "mainG": (12, -10),
            "coudeD": (-3, -11), "mainD": (2, -10), "genouG": (-4, 6), "piedG": (-6, 11), "genouD": (4, 6), "piedD": (6, 11)},
    "tombe": {"tete": (-8, -6), "cou": (-5, -4), "hanche": (2, 0), "coudeG": (-8, 0), "mainG": (-11, 3),
              "coudeD": (-3, 1), "mainD": (-1, 4), "genouG": (7, -1), "piedG": (12, 3), "genouD": (6, 3), "piedD": (11, 6)},
}


def heros(t, pose, x, y, s=1.0, couleur=OMBRE, liseres=((OR_PALE, 0, -1),), retourne=False, cape=False, epais=0):
    """Silhouette en pose `pose`, hanche en (x, y), échelle s ; liserés de lumière (couleur, dx, dy)"""
    p = POSES[pose] if isinstance(pose, str) else pose
    k = -1 if retourne else 1
    q = {n: (x + k * px * s, y + py * s) for n, (px, py) in p.items()}
    if cape:
        cx, cy = q["cou"]
        q["cape"] = [(cx + k * 3 * s - k * 5 * s, y + 9 * s), (cx - k * 9 * s, y + 7 * s)]
    m = t.masque(lambda d: silhouette(d, q, s))
    if s > 1.25 or epais:
        m = dilater(m, epais or 1)
    t.peindre(m, couleur)
    for c, dx, dy in liseres:
        t.liseré(m, c, dx, dy)
    return m, q


# --- Armes -----------------------------------------------------------------------------------

def lame(t, x0, y0, x1, y1, metal=ACIER, garde=True, largeur=2, poignee=BOIS, lueur=None):
    """Épée de la poignée (x0, y0) à la pointe (x1, y1)"""
    a = math.atan2(y1 - y0, x1 - x0)
    ux, uy = math.cos(a), math.sin(a)
    gx, gy = x0 + ux * 4, y0 + uy * 4
    if lueur:
        t.lueur((gx + x1) / 2, (gy + y1) / 2, math.hypot(x1 - gx, y1 - gy) * 0.6, lueur, 0.6)
    m_lame = t.masque(lambda d: d.line([(gx, gy), (x1, y1)], fill=1, width=largeur))
    t.objet(m_lame, metal, BLANC, melange(metal, (30, 30, 60), 0.5))
    m_p = t.masque(lambda d: d.line([(x0, y0), (gx, gy)], fill=1, width=2))
    t.objet(m_p & ~m_lame, poignee, melange(poignee, BLANC, 0.3), melange(poignee, (0, 0, 0), 0.5))
    if garde:
        nx, ny = -uy, ux
        m_g = t.masque(lambda d: d.line([(gx + nx * 3, gy + ny * 3), (gx - nx * 3, gy - ny * 3)], fill=1, width=2))
        t.objet(m_g, OR, OR_PALE, (150, 90, 30))


def sceptre(t, x0, y0, x1, y1, or_=OR, gemme=(255, 250, 220)):
    """Sceptre doré : hampe de (x0, y0) à la tête ouvragée en (x1, y1)"""
    m = t.masque(lambda d: d.line([(x0, y0), (x1, y1)], fill=1, width=2))
    t.objet(m, or_, OR_PALE, (160, 100, 30))
    a = math.atan2(y1 - y0, x1 - x0)
    ux, uy, nx, ny = math.cos(a), math.sin(a), -math.sin(a), math.cos(a)
    couronne = t.masque(lambda d: [d.line([(x1, y1), (x1 + ux * 4 + nx * c * 3, y1 + uy * 4 + ny * c * 3)], fill=1, width=1)
                                   for c in (-1, 0, 1)])
    t.objet(couronne, or_, OR_PALE, (160, 100, 30))
    orbe(t, x1 + ux * 3, y1 + uy * 3, 2, gemme)


def hache(t, x0, y0, x1, y1, metal=ACIER, double=False):
    a = math.atan2(y1 - y0, x1 - x0)
    ux, uy, nx, ny = math.cos(a), math.sin(a), -math.sin(a), math.cos(a)
    m_m = t.masque(lambda d: d.line([(x0, y0), (x1, y1)], fill=1, width=2))
    t.objet(m_m, BOIS, melange(BOIS, BLANC, 0.3), melange(BOIS, (0, 0, 0), 0.5))
    def fer(sg):
        bx, by = x1 - ux * 2, y1 - uy * 2
        return [(bx, by), (bx + sg * nx * 9 - ux * 4, by + sg * ny * 9 - uy * 4),
                (bx + sg * nx * 9 + ux * 5, by + sg * ny * 9 + uy * 5), (bx + ux * 3, by + uy * 3)]
    m_f = t.masque(lambda d: [d.polygon(fer(sg), fill=1) for sg in ((1, -1) if double else (1,))])
    t.objet(m_f, metal, BLANC, melange(metal, (30, 30, 60), 0.5))


def baton(t, x0, y0, x1, y1, bois=(100, 70, 50), gemme=None):
    m = t.masque(lambda d: d.line([(x0, y0), (x1, y1)], fill=1, width=2))
    t.objet(m, bois, melange(bois, BLANC, 0.3), melange(bois, (0, 0, 0), 0.5))
    if gemme:
        orbe(t, x1, y1, 3, gemme)


def bouclier(t, cx, cy, r, champ=(60, 110, 220), bord=ACIER, embleme=OR):
    m = t.masque(lambda d: d.polygon([(cx - r, cy - r), (cx + r, cy - r), (cx + r, cy + r * 0.2), (cx, cy + r * 1.3),
                                      (cx - r, cy + r * 0.2)], fill=1))
    t.objet(m, champ, melange(champ, BLANC, 0.4), melange(champ, (0, 0, 0), 0.5))
    if embleme:
        e = t.masque(lambda d: (d.line([(cx, cy - r + 2), (cx, cy + r * 0.9)], fill=1, width=2),
                                d.line([(cx - r + 2, cy - r * 0.3), (cx + r - 2, cy - r * 0.3)], fill=1, width=2)))
        t.peindre(e & m, embleme)
    bord_m = dilater(m, 1) & ~m
    t.peindre(bord_m, bord)
    t.contour(dilater(m, 1), (12, 8, 16))
    return m


def arc_tir(t, cx, cy, h, bois=BOIS, corde=BLANC, vers=1):
    """Arc vertical de hauteur h, ventre tourné vers `vers` (1 = droite)"""
    x0 = cx - vers * h * 0.3
    m = t.masque(lambda d: d.arc([cx - h * 0.35 - (h * 0.35 if vers < 0 else 0), cy - h / 2,
                                  cx + h * 0.35 - (h * 0.35 if vers < 0 else 0), cy + h / 2],
                                 -80 if vers > 0 else 100, 80 if vers > 0 else 260, fill=1, width=2))
    t.objet(m, bois, melange(bois, BLANC, 0.3), melange(bois, (0, 0, 0), 0.5))
    t.peindre(t.masque(lambda d: d.line([(cx, cy - h / 2 + 1), (cx, cy + h / 2 - 1)], fill=1)), corde)


def arbalete(t, x, y, couleur=(110, 70, 40), metal=ACIER, vers=1):
    """Arbalète pointant vers `vers`, crosse en (x, y)"""
    m = t.masque(lambda d: d.line([(x, y), (x + vers * 14, y)], fill=1, width=3))
    t.objet(m, couleur, melange(couleur, BLANC, 0.3), melange(couleur, (0, 0, 0), 0.5))
    a = t.masque(lambda d: d.arc([x + vers * 10 - 3, y - 7, x + vers * 10 + 3, y + 7],
                                 -90 if vers > 0 else 90, 90 if vers > 0 else 270, fill=1, width=2))
    t.objet(a, metal, BLANC, melange(metal, (30, 30, 60), 0.5))


def fleche(t, x0, y0, x1, y1, couleur=ACIER, trainee=None, fut=BOIS, plumes=BLANC, epais=1):
    if trainee:
        t.peindre(t.masque(lambda d: d.line([(x0 - (x1 - x0) * 0.6, y0 - (y1 - y0) * 0.6), (x0, y0)], fill=1, width=epais + 1)),
                  trainee)
    t.peindre(t.masque(lambda d: d.line([(x0, y0), (x1, y1)], fill=1, width=epais)), fut)
    a = math.atan2(y1 - y0, x1 - x0)
    ux, uy, nx, ny = math.cos(a), math.sin(a), -math.sin(a), math.cos(a)
    pointe = t.masque(lambda d: d.polygon([(x1 + ux * 3, y1 + uy * 3), (x1 + nx * 2, y1 + ny * 2), (x1 - nx * 2, y1 - ny * 2)], fill=1))
    t.peindre(pointe, couleur)
    t.peindre(t.masque(lambda d: d.line([(x0 + nx * 2, y0 + ny * 2), (x0 + ux * 2, y0 + uy * 2), (x0 - nx * 2, y0 - ny * 2)],
                                        fill=1)), plumes)


# --- Magie et matière ------------------------------------------------------------------------

def orbe(t, cx, cy, r, couleur, halo=True):
    if halo:
        t.lueur(cx, cy, r * 2.6, couleur, 0.8)
    m = t.masque(lambda d: d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=1))
    t.objet(m, couleur, melange(couleur, BLANC, 0.6), melange(couleur, (0, 0, 0), 0.4), contour=melange(couleur, (0, 0, 0), 0.75))
    t.point(cx - r * 0.35, cy - r * 0.35, BLANC)


def boule_de_feu(t, cx, cy, r, vers=(-1, 0), feu=((255, 250, 200), (255, 190, 40), (240, 80, 20))):
    vx, vy = vers
    for i, (k, c) in enumerate(((2.6, feu[2]), (1.9, feu[1]))):
        m = t.masque(lambda d, k=k: d.polygon([(cx + vx * r * k * 1.6, cy + vy * r * k * 1.6),
                                                (cx - vy * r * k * 0.6, cy + vx * r * k * 0.6),
                                                (cx + vy * r * k * 0.6, cy - vx * r * k * 0.6)], fill=1))
        t.peindre(m, c)
    t.lueur(cx, cy, r * 3, feu[1], 0.7)
    for k, c in ((1.0, feu[2]), (0.75, feu[1]), (0.45, feu[0])):
        t.peindre(t.masque(lambda d, k=k: d.ellipse([cx - r * k, cy - r * k, cx + r * k, cy + r * k], fill=1)), c)


def explosion(t, cx, cy, r, couleurs=((255, 250, 210), (255, 200, 60), (240, 90, 30)), branches=9):
    t.lueur(cx, cy, r * 1.8, couleurs[1], 0.9)
    for k, c in zip((1.0, 0.7, 0.4), (couleurs[2], couleurs[1], couleurs[0])):
        pts = []
        for i in range(branches * 2):
            a = math.radians(i * 180 / branches + 7)
            rr = r * k * (1 if i % 2 == 0 else 0.55)
            pts.append((cx + rr * math.cos(a), cy + rr * math.sin(a)))
        t.peindre(t.masque(lambda d, pts=pts: d.polygon(pts, fill=1)), c)


def nuage(t, cx, cy, w, h, couleur, sombre=None, n=6, graine=1, alpha=None):
    import random
    rnd = random.Random(graine)
    bulles = [(cx + rnd.uniform(-w / 2, w / 2) * 0.8, cy + rnd.uniform(-h / 2, h / 2) * 0.6, rnd.uniform(h * 0.35, h * 0.6))
              for _ in range(n)]
    m = t.masque(lambda d: [d.ellipse([x - r, y - r, x + r, y + r], fill=1) for x, y, r in bulles])
    if alpha:
        t.voile(m, couleur, alpha)
    else:
        t.objet(m, couleur, melange(couleur, BLANC, 0.35), sombre or melange(couleur, (0, 0, 0), 0.4),
                contour=melange(couleur, (0, 0, 0), 0.7))
    return m


def eclair(t, pts, couleur=(200, 240, 255), coeur=BLANC, halo=True):
    if halo:
        for x, y in pts[1:-1]:
            t.lueur(x, y, 6, couleur, 0.35)
    t.peindre(t.masque(lambda d: d.line(pts, fill=1, width=2)), couleur)
    t.peindre(t.masque(lambda d: d.line(pts, fill=1, width=1)), coeur)


def zigzag(x0, y0, x1, y1, n=5, amp=3, graine=2):
    import random
    rnd = random.Random(graine)
    return [(x0 + (x1 - x0) * i / n + (rnd.uniform(-amp, amp) if 0 < i < n else 0),
             y0 + (y1 - y0) * i / n + (rnd.uniform(-amp, amp) if 0 < i < n else 0)) for i in range(n + 1)]


def tornade(t, cx, bas, haut, largeur, couleur, clair=BLANC, n=6):
    for i in range(n):
        f = i / (n - 1)
        y = bas - (bas - haut) * f
        w = 3 + largeur * f
        m = t.masque(lambda d, y=y, w=w: d.arc([cx - w + math.sin(f * 5) * 2, y - 2.5, cx + w + math.sin(f * 5) * 2, y + 2.5],
                                                 200, 520, fill=1, width=2))
        t.peindre(m, couleur if i % 2 else clair)


def croissant(t, cx, cy, r, debut, fin, couleur, epais=3, clair=BLANC):
    """Arc de taille (coup d'arme, lame d'air)"""
    t.peindre(t.masque(lambda d: d.arc([cx - r, cy - r, cx + r, cy + r], debut, fin, fill=1, width=epais)), couleur)
    t.peindre(t.masque(lambda d: d.arc([cx - r, cy - r, cx + r, cy + r], debut + (fin - debut) * 0.3, fin, fill=1, width=1)),
              clair)


def anneau(t, cx, cy, rx, ry, couleur, epais=1, clair=None):
    t.peindre(t.masque(lambda d: d.ellipse([cx - rx, cy - ry, cx + rx, cy + ry], outline=1, width=epais)), couleur)
    if clair:
        t.peindre(t.masque(lambda d: d.arc([cx - rx, cy - ry, cx + rx, cy + ry], 200, 290, fill=1, width=1)), clair)


def ondes(t, cx, cy, couleur, rayons=(6, 11, 16), debut=-40, fin=40):
    for r in rayons:
        t.peindre(t.masque(lambda d, r=r: d.arc([cx - r, cy - r, cx + r, cy + r], debut, fin, fill=1, width=2)), couleur)


def portail(t, cx, cy, rx, ry, bord, fond, lueur=None):
    if lueur:
        t.lueur(cx, cy - ry, rx * 1.3, lueur, 0.6, sy=1.6)
    m = t.masque(lambda d: d.ellipse([cx - rx, cy - ry, cx + rx, cy + ry], fill=1))
    t.peindre(m, fond)
    anneau(t, cx, cy, rx, ry, bord, 2, clair=melange(bord, BLANC, 0.5))


def sang(t, gouttes, couleur=SANG):
    for x, y, r in gouttes:
        m = t.masque(lambda d, x=x, y=y, r=r: d.ellipse([x - r, y - r, x + r, y + r], fill=1))
        t.peindre(m, couleur)
        t.point(x - r * 0.3, y - r * 0.3, melange(couleur, BLANC, 0.5))


def griffures(t, x, y, couleur=(255, 240, 230), sombre=SANG, n=3, long=18, angle=-60, ecart=5):
    a = math.radians(angle)
    ux, uy, nx, ny = math.cos(a), math.sin(a), -math.sin(a), math.cos(a)
    for i in range(n):
        o = (i - (n - 1) / 2) * ecart
        x0, y0 = x + nx * o - ux * long / 2, y + ny * o - uy * long / 2
        x1, y1 = x + nx * o + ux * long / 2, y + ny * o + uy * long / 2
        t.peindre(t.masque(lambda d: d.line([(x0, y0), (x1, y1)], fill=1, width=2)), sombre)
        t.peindre(t.masque(lambda d: d.line([(x0, y0), (x1, y1)], fill=1, width=1)), couleur)


def crane(t, cx, cy, r, os=(230, 230, 200), yeux=(120, 255, 140), sombre=(30, 20, 30)):
    m = t.masque(lambda d: (d.ellipse([cx - r, cy - r, cx + r, cy + r * 0.8], fill=1),
                            d.rectangle([cx - r * 0.55, cy + r * 0.4, cx + r * 0.55, cy + r * 1.2], fill=1)))
    t.objet(m, os, BLANC, melange(os, (0, 0, 0), 0.4), contour=sombre)
    for sx in (-1, 1):
        e = t.masque(lambda d, sx=sx: d.ellipse([cx + sx * r * 0.42 - r * 0.28, cy - r * 0.05, cx + sx * r * 0.42 + r * 0.28,
                                                  cy + r * 0.5], fill=1))
        t.peindre(e, yeux)


def pics(t, x0, x1, sol, hauteur, couleur=(230, 225, 200), n=5, sombre=None):
    larg = (x1 - x0) / n
    m = t.masque(lambda d: [d.polygon([(x0 + i * larg, sol), (x0 + (i + 0.5) * larg, sol - hauteur * (0.7 + 0.3 * (i % 2))),
                                       (x0 + (i + 1) * larg, sol)], fill=1) for i in range(n)])
    t.objet(m, couleur, BLANC, sombre or melange(couleur, (0, 0, 0), 0.45))


def mur(t, x0, y0, x1, y1, pierre=(130, 120, 110), joint=None, rougeoie=None):
    m = t.masque(lambda d: d.rectangle([x0, y0, x1, y1], fill=1))
    t.objet(m, pierre, melange(pierre, BLANC, 0.3), melange(pierre, (0, 0, 0), 0.45))
    j = joint or melange(pierre, (0, 0, 0), 0.4)
    for y in range(int(y0) + 4, int(y1), 4):
        t.peindre(t.masque(lambda d, y=y: d.line([(x0 + 1, y), (x1 - 1, y)], fill=1)), j)
        for x in range(int(x0) + (3 if (y // 4) % 2 else 6), int(x1), 6):
            t.peindre(t.masque(lambda d, x=x, y=y: d.line([(x, y - 3), (x, y)], fill=1)), j)
    if rougeoie:
        t.lueur((x0 + x1) / 2, y1, (x1 - x0) * 0.7, rougeoie, 0.6, sy=0.6)


def plume(t, x, y, angle, couleur=BLANC, long=7):
    a = math.radians(angle)
    ux, uy, nx, ny = math.cos(a), math.sin(a), -math.sin(a), math.cos(a)
    m = t.masque(lambda d: d.polygon([(x, y), (x + ux * long / 2 + nx * 2, y + uy * long / 2 + ny * 2), (x + ux * long, y + uy * long),
                                      (x + ux * long / 2 - nx * 2, y + uy * long / 2 - ny * 2)], fill=1))
    t.objet(m, couleur, BLANC, melange(couleur, (0, 0, 0), 0.35), contour=melange(couleur, (0, 0, 0), 0.7))


def oeil(t, cx, cy, r, iris=(255, 60, 40), blanc=(250, 240, 220)):
    m = t.masque(lambda d: d.ellipse([cx - r, cy - r * 0.55, cx + r, cy + r * 0.55], fill=1))
    t.objet(m, blanc, BLANC, melange(blanc, (0, 0, 0), 0.3))
    t.peindre(t.masque(lambda d: d.ellipse([cx - r * 0.45, cy - r * 0.45, cx + r * 0.45, cy + r * 0.45], fill=1)) & m, iris)
    t.peindre(t.masque(lambda d: d.ellipse([cx - 1, cy - 1, cx + 1, cy + 1], fill=1)), (10, 5, 10))


def mire(t, cx, cy, r, couleur=(255, 60, 50)):
    anneau(t, cx, cy, r, r, couleur, 1)
    for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        t.peindre(t.masque(lambda d, dx=dx, dy=dy: d.line([(cx + dx * (r - 3), cy + dy * (r - 3)), (cx + dx * (r + 3), cy + dy * (r + 3))],
                                                           fill=1)), couleur)


def fissure(t, x0, y0, x1, y1, couleur=CYAN, sombre=(10, 4, 20), largeur=3, graine=3):
    pts = zigzag(x0, y0, x1, y1, n=6, amp=2.5, graine=graine)
    t.peindre(t.masque(lambda d: d.line(pts, fill=1, width=largeur + 2)), sombre)
    t.peindre(t.masque(lambda d: d.line(pts, fill=1, width=1)), couleur)


def cristaux(t, pts, couleur=CYAN, sombre=VIOLET):
    for x, y, h in pts:
        m = t.masque(lambda d, x=x, y=y, h=h: d.polygon([(x, y - h), (x + h * 0.35, y), (x, y + h * 0.2), (x - h * 0.35, y)], fill=1))
        t.objet(m, couleur, BLANC, sombre)


def marque_etherium(t, x=40, y=8):
    """Petite étincelle d'Étherium des ultimes"""
    t.etincelle(x, y, CYAN, grande=True)


def bete(t, espece, x, y, s=1.0, couleur=OMBRE, liseres=((OR_PALE, 0, -1),), retourne=False):
    """Silhouettes de créatures, (x, y) au milieu du corps ; les quadrupèdes regardent à droite"""
    k = -1 if retourne else 1
    P = lambda px, py: (x + k * px * s, y + py * s)
    poly = lambda d, pts: d.polygon([P(*p) for p in pts], fill=1)
    ell = lambda d, x0, y0, x1, y1: d.ellipse(sorted_box(P(x0, y0), P(x1, y1)), fill=1)
    ligne = lambda d, pts, w: d.line([P(*p) for p in pts], fill=1, width=max(1, round(w * s)))

    def dessin(d):
        if espece == "loup":
            ell(d, -10, -4, 8, 4)
            poly(d, [(4, -3), (8, -10), (12, -9), (9, 1)])
            ell(d, 7, -13, 14, -7)
            poly(d, [(12, -12), (19, -9), (12, -7)])
            poly(d, [(8, -11), (9, -16), (11, -12)])
            poly(d, [(-9, -2), (-16, -9), (-18, -6), (-10, 1)])
            for a, b in (((-8, 2), (-9, 11)), ((-5, 2), (-4, 11)), ((4, 2), (3, 11)), ((7, 2), (8, 11))):
                ligne(d, [a, b], 2)
        elif espece in ("ours", "hibours"):
            ell(d, -12, -7, 10, 6)
            ell(d, 7, -7, 16, 2)
            ell(d, 13, -4, 19, 1)
            ell(d, 8, -9, 12, -5)
            for a, b in (((-9, 3), (-9, 11)), ((-4, 3), (-4, 11)), ((3, 3), (3, 11)), ((7, 2), (8, 11))):
                ligne(d, [a, b], 3)
            if espece == "hibours":
                poly(d, [(9, -7), (9, -13), (12, -7)])
                poly(d, [(17, -3), (21, -1), (17, 0)])
        elif espece == "cerf":
            ell(d, -10, -4, 8, 3)
            poly(d, [(4, -2), (8, -12), (11, -11), (9, 1)])
            ell(d, 7, -16, 12, -11)
            poly(d, [(10, -15), (16, -13), (11, -11)])
            for b in (-1, 1):
                ligne(d, [(9, -15), (7 + b, -20), (4 + b * 2, -25)], 1)
                ligne(d, [(8 + b, -19), (12 + b, -23)], 1)
                ligne(d, [(6 + b, -22), (1 + b, -24)], 1)
            for a, b in (((-8, 1), (-9, 12)), ((-5, 1), (-4, 12)), ((4, 1), (3, 12)), ((7, 1), (8, 12))):
                ligne(d, [a, b], 1.5)
        elif espece in ("oiseau", "griffon"):
            g = 1.3 if espece == "griffon" else 1
            ell(d, -2.5 * g, -6, 2.5 * g, 6 * g)
            ell(d, -2, -10, 2, -5)
            poly(d, [(1, -9), (4, -7), (1, -6)])
            for c in (-1, 1):
                poly(d, [(c * 1, -3), (c * 16, -12), (c * 15, -7), (c * 11, -5), (c * 12, -1), (c * 7, 0), (c * 1, 3)])
            poly(d, [(-2, 5 * g), (-4, 11 * g), (4, 11 * g), (2, 5 * g)])
            if espece == "griffon":
                ligne(d, [(-2, 6), (-4, 12)], 2)
                ligne(d, [(2, 6), (4, 12)], 2)
        elif espece == "squelette":
            silhouette(d, {n: P(px, py) for n, (px, py) in POSES["brasOuverts"].items()}, s)
        elif espece == "golem":
            ell(d, -9, -12, 9, 6)
            ell(d, -3, -17, 3, -11)
            ligne(d, [(-8, -8), (-12, 4)], 4)
            ligne(d, [(8, -8), (12, 4)], 4)
            ligne(d, [(-4, 4), (-5, 12)], 4)
            ligne(d, [(4, 4), (5, 12)], 4)

    m = t.masque(dessin)
    if s > 1.3:
        m = dilater(m, 1)
    t.peindre(m, couleur)
    for c, dx, dy in liseres:
        t.liseré(m, c, dx, dy)
    return m


def sorted_box(a, b):
    return [min(a[0], b[0]), min(a[1], b[1]), max(a[0], b[0]), max(a[1], b[1])]
