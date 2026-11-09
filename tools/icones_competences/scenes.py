"""Outils pour composer les icônes en petites scènes de pixel art (48×48).

Chaque compétence a sa scène, avec sa propre ambiance de couleur : le fond raconte où l'on est
(ciel d'orage, nuit, lumière sacrée), les formes racontent ce qui se passe. Les dégradés sont
tramés par bandes, les formes sont posées sans lissage, et l'image finale est réduite à une
petite palette : c'est ce qui donne l'aspect pixel art.
"""
import math

import numpy as np
from PIL import Image, ImageDraw

N = 48
BAYER4 = np.array([[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]) / 16 - 0.47


def rgb(c):
    return np.array(c, dtype=float) / 255


def melange(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


class Toile:
    def __init__(self):
        self.c = np.zeros((N, N, 3))
        ys, xs = np.mgrid[0:N, 0:N]
        self.x, self.y = xs + 0.5, ys + 0.5
        self.trame = np.tile(BAYER4, (N // 4, N // 4))

    # --- Dégradés et lumière -------------------------------------------------------------

    def _bandes(self, t, couleurs, bandes):
        """t ∈ [0, 1] -> couleur, en `bandes` paliers tramés"""
        t = np.clip(t, 0, 1)
        q = np.clip(np.floor(t * bandes + self.trame * 0.55 + 0.5) - 0.5 + 0.5, 0, bandes) / bandes
        pts = np.linspace(0, 1, len(couleurs))
        cols = np.array([rgb(c) for c in couleurs])
        return np.stack([np.interp(q, pts, cols[:, i]) for i in range(3)], axis=-1)

    def fond_radial(self, cx, cy, rayon, couleurs, bandes=6):
        t = np.hypot(self.x - cx, self.y - cy) / rayon
        self.c = self._bandes(t, couleurs, bandes)

    def fond_vertical(self, couleurs, haut=0, bas=N, bandes=7):
        t = (self.y - haut) / (bas - haut)
        self.c = self._bandes(t, couleurs, bandes)

    def zone_verticale(self, masque, couleurs, haut, bas, bandes=4):
        t = (self.y - haut) / (bas - haut)
        self.c = np.where(masque[..., None], self._bandes(t, couleurs, bandes), self.c)

    def lueur(self, cx, cy, rayon, couleur, force=1.0, bandes=4, sy=1.0):
        """Lumière ajoutée, qui décroît par paliers tramés"""
        d = np.hypot(self.x - cx, (self.y - cy) / sy) / rayon
        t = np.clip(1 - d, 0, 1) ** 1.6
        q = np.clip(np.floor(t * bandes + self.trame * 0.9), 0, bandes) / bandes
        self.c = np.clip(self.c + q[..., None] * rgb(couleur) * force, 0, 1)

    def voile(self, masque, couleur, alpha):
        """Couleur translucide sur un masque"""
        m = masque[..., None]
        self.c = np.where(m, self.c * (1 - alpha) + rgb(couleur) * alpha, self.c)

    def vignette(self, force=0.55):
        d = np.hypot(self.x - N / 2, self.y - N / 2) / (N * 0.68)
        t = np.clip(d - 0.35, 0, 1) / 0.65
        q = np.clip(np.floor(t * 3 + self.trame), 0, 3) / 3
        self.c *= (1 - q * force)[..., None]

    # --- Formes --------------------------------------------------------------------------

    def masque(self, dessin):
        """Masque booléen dessiné sans lissage : dessin(ImageDraw)"""
        m = Image.new("1", (N, N), 0)
        dessin(ImageDraw.Draw(m))
        return np.array(m, dtype=bool)

    def peindre(self, masque, couleur):
        self.c = np.where(masque[..., None], rgb(couleur), self.c)

    def contour(self, masque, couleur, epais=1):
        bord = dilater(masque, epais) & ~masque
        self.peindre(bord, couleur)

    def liseré(self, masque, couleur, dx, dy):
        """Pixels du masque éclairés par une lumière venant de (dx, dy)"""
        voisin = np.roll(np.roll(masque, dy, axis=0), dx, axis=1)
        self.peindre(masque & ~voisin, couleur)

    def objet(self, masque, couleur, clair, sombre, contour=(12, 8, 16), lumiere=(-1, -1)):
        """Remplit, éclaire d'un côté, assombrit de l'autre, puis cerne"""
        self.peindre(masque, couleur)
        self.liseré(masque, sombre, -lumiere[0], -lumiere[1])
        self.liseré(masque, clair, lumiere[0], lumiere[1])
        self.contour(masque, contour)

    def point(self, x, y, couleur):
        if 0 <= x < N and 0 <= y < N:
            self.c[int(y), int(x)] = rgb(couleur)

    def etincelle(self, x, y, couleur, grande=False, coeur=(255, 255, 255)):
        for dx, dy in ((0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)):
            self.point(x + dx, y + dy, couleur)
        if grande:
            for dx, dy in ((2, 0), (-2, 0), (0, 2), (0, -2)):
                self.point(x + dx, y + dy, couleur)
        self.point(x, y, coeur)

    def rayons(self, cx, cy, r0, r1, n, couleur, decalage=0, largeur=1):
        m = self.masque(lambda d: [d.line([(cx + r0 * math.cos(a), cy + r0 * math.sin(a)),
                                           (cx + r1 * math.cos(a), cy + r1 * math.sin(a))], fill=1, width=largeur)
                                   for a in (math.radians(decalage + i * 360 / n) for i in range(n))])
        self.peindre(m, couleur)

    # --- Fin -----------------------------------------------------------------------------

    def cadre(self, ultime=False):
        noir = rgb((8, 6, 12))
        self.c[0, :] = self.c[-1, :] = self.c[:, 0] = self.c[:, -1] = noir
        if ultime:
            violet, cyan = rgb((140, 60, 220)), rgb((110, 245, 255))
            self.c[1, 1:-1] = self.c[-2, 1:-1] = violet
            self.c[1:-1, 1] = self.c[1:-1, -2] = violet
            for x, y in ((2, 2), (N - 3, 2), (2, N - 3), (N - 3, N - 3)):
                for dx, dy in ((0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)):
                    self.c[y + dy, x + dx] = cyan
        else:
            self.c[1, 1:-1] = np.clip(self.c[1, 1:-1] * 1.5 + 0.08, 0, 1)
            self.c[1:-1, 1] = np.clip(self.c[1:-1, 1] * 1.5 + 0.08, 0, 1)
            self.c[-2, 1:-1] *= 0.45
            self.c[1:-1, -2] *= 0.45

    def image(self, couleurs=40):
        img = Image.fromarray((np.clip(self.c, 0, 1) * 255).round().astype(np.uint8), "RGB")
        img = img.quantize(couleurs, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE).convert("RGBA")
        return img


def dilater(m, n=1):
    r = m.copy()
    for _ in range(n):
        r = r | np.roll(r, 1, 0) | np.roll(r, -1, 0) | np.roll(r, 1, 1) | np.roll(r, -1, 1)
    return r


# --- Pièces réutilisables ----------------------------------------------------------------

def silhouette(d, pose, s=1.0):
    """Une silhouette de héros à partir de ses articulations (dict de points) ; `s` règle
    l'épaisseur des membres pour que bras et jambes restent séparés même en petit"""
    p = pose
    tx, ty = p["cou"]
    hx, hy = p["hanche"]
    bras, jambe = max(2, round(2.2 * s)), max(2, round(2.6 * s))
    # buste
    d.polygon([(tx - 3.2 * s, ty), (tx + 3.2 * s, ty), (hx + 2.2 * s, hy), (hx - 2.2 * s, hy)], fill=1)
    # tête
    cx, cy = p["tete"]
    r = max(2.2, 2.7 * s)
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=1)
    d.line([(cx, cy), (tx, ty)], fill=1, width=max(2, round(2 * s)))
    # bras et jambes
    for epaule, coude, main in (((tx - 2.6 * s, ty + 1), p["coudeG"], p["mainG"]), ((tx + 2.6 * s, ty + 1), p["coudeD"], p["mainD"])):
        d.line([epaule, coude, main], fill=1, width=bras, joint="curve")
    for genou, pied, cote in ((p["genouG"], p["piedG"], -1), (p["genouD"], p["piedD"], 1)):
        d.line([(hx + cote * 1.4 * s, hy), genou, pied], fill=1, width=jambe, joint="curve")
    if p.get("cape"):
        d.polygon([(tx - 3 * s, ty), (tx + 3 * s, ty)] + p["cape"], fill=1)


def maillet(d, x0, y0, x1, y1, tete=(9, 6), manche=2):
    """Manche de (x0, y0) à (x1, y1), tête centrée en (x1, y1), perpendiculaire au manche"""
    a = math.atan2(y1 - y0, x1 - x0)
    manche_m = [(x0, y0), (x1, y1)]
    w, h = tete
    ux, uy = math.cos(a), math.sin(a)
    nx, ny = -uy, ux
    tete_m = [(x1 + nx * w / 2 - ux * h / 2, y1 + ny * w / 2 - uy * h / 2),
              (x1 + nx * w / 2 + ux * h / 2, y1 + ny * w / 2 + uy * h / 2),
              (x1 - nx * w / 2 + ux * h / 2, y1 - ny * w / 2 + uy * h / 2),
              (x1 - nx * w / 2 - ux * h / 2, y1 - ny * w / 2 - uy * h / 2)]
    return manche_m, tete_m


def dessiner_maillet(t, x0, y0, x1, y1, tete=(10, 6), bois=(150, 92, 44), metal=(206, 214, 228),
                     lumiere=(-1, -1)):
    manche, tete_p = maillet(None, x0, y0, x1, y1, tete)
    m_manche = t.masque(lambda d: d.line(manche, fill=1, width=2))
    m_tete = t.masque(lambda d: d.polygon(tete_p, fill=1))
    t.objet(m_manche & ~m_tete, bois, melange(bois, (255, 230, 180), 0.4), melange(bois, (0, 0, 0), 0.5), lumiere=lumiere)
    t.objet(m_tete, metal, (255, 255, 255), melange(metal, (40, 40, 80), 0.55), lumiere=lumiere)
    # frettes dorées aux deux bouts de la tête et au cœur : un maillet, pas un panneau
    a = math.atan2(y1 - y0, x1 - x0)
    ux, uy, nx, ny = math.cos(a), math.sin(a), -math.sin(a), math.cos(a)
    w, h = tete
    for k in (-0.32, 0.32):
        cx, cy = x1 + nx * w * k, y1 + ny * w * k
        bande = t.masque(lambda d, cx=cx, cy=cy: d.line([(cx - ux * h * 0.45, cy - uy * h * 0.45), (cx + ux * h * 0.45, cy + uy * h * 0.45)],
                                                         fill=1, width=1))
        t.peindre(bande & m_tete, (230, 170, 60))
    t.peindre(t.masque(lambda d: d.ellipse([x1 - 1, y1 - 1, x1 + 1, y1 + 1], fill=1)) & m_tete, melange(metal, (40, 40, 80), 0.6))
    return m_manche | m_tete
