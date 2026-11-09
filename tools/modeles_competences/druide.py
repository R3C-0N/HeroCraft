"""Les formes animales du Druide : loup, ours runique, grand cerf, faucon, puis les hybrides faits
de leurs pièces (ours-hibou, wendigo, griffon) et la chimère de l'ultime.

Os des quadrupèdes : `root` (le corps), `head`, `tail`, `legFR`, `legFL`, `legBR`, `legBL` (avant ou
arrière, droite ou gauche). Les oiseaux et les ailés ont en plus `wingR` et `wingL`.
"""
from boites import Model

# Lueurs du druide
SAP = "#7dff8a"
SAP_DEEP = "#2fbf5a"
LEAF = "#4f8a2e"
LEAF_DARK = "#3a6b22"
MOSS = "#5f7f2f"
BARK = "#6b4a2f"


def eyes(color, x0, y, gap, w=1, h=1, pupil=None):
    """Deux yeux sur la face avant d'une tête de `gap` texels d'écart (symétriques)."""
    out = [("mirror", ("rect", x0, y, w, h, color))]
    if pupil:
        out.append(("mirror", ("px", x0, y, pupil)))
    return out


# ------------------------------------------------------------------ pièces partagées


def quadruped_legs(m, xs, zf, zb, top, h, w, color, pattern="fur", hoof=None, claws=None, wb=None):
    """Quatre pattes : os `legFR`… au haut de chaque patte, pavé de `w` de côté et de hauteur `h`."""
    wb = wb or w
    for name, x, z, ww in (("legFR", -xs, zf, w), ("legFL", xs, zf, w), ("legBR", -xs, zb, wb), ("legBL", xs, zb, wb)):
        m.bone(name, "root", (x, top, z))
        dec = {}
        if hoof:
            dec["sides"] = [("rect", 0, -2, 99, 2, hoof)]
            dec["bottom"] = [("rect", 0, 0, 99, 99, hoof)]
        if claws:
            dec["front"] = [("pixels", [(i, -1) for i in range(0, int(ww), 2)], claws)]
        m.box(name, (x - ww / 2, top - h, z - ww / 2), (x + ww / 2, top, z + ww / 2), color, pattern, dec)


def wolf_head(m, bone, base, at, fur, light, eye):
    """Tête de loup : crâne, museau, truffe, oreilles. `at` : coin (x centre, bas, avant du crâne)."""
    x, y, z = at
    m.box(bone, (x - 3, y, z - 6), (x + 3, y + 6, z), fur, "fur", {
        "front": eyes(eye, 0, 2, 0, 2, 1, "#20140a") + [("rect", 2, 4, 2, 2, light)]})
    m.box(bone, (x - 1.5, y, z), (x + 1.5, y + 3, z + 4), light, "fur", {
        "front": [("rect", 0, 0, 3, 1, "#1c1714")], "top": [("rect", 0, -1, 3, 1, "#1c1714")],
        "bottom": [("rect", 0, 0, 3, 99, shade_hex(light, -0.2))]})
    for s in (-1, 1):
        m.box(bone, (x + s * 3 - (2 if s > 0 else 0), y + 6, z - 4), (x + s * 3 + (0 if s > 0 else 2), y + 9, z - 2.5),
              fur, "fur", {"front": [("rect", 0, 1, 2, 2, "#3b2e2a")]})


def shade_hex(c, k):
    from boites import rgb, shade
    r, g, b, a = shade(rgb(c), k)
    return "#%02x%02x%02x" % (r, g, b)


def antlers(m, bone, root, spread, height, color, tip=None, moss=None, thick=1.4, scale=1.0):
    """Bois de cerf : une perche de chaque côté, trois andouillers, du `root` (x centre, y, z)."""
    x, y, z = root
    for s in (-1, 1):
        base = (x + s * 2, y, z)
        a = (x + s * (2 + 3 * spread * scale), y + 6 * height * scale, z - 1.5 * scale)
        b = (x + s * (2 + 6 * spread * scale), y + 12 * height * scale, z - 3.5 * scale)
        c = (x + s * (2 + 8 * spread * scale), y + 17 * height * scale, z - 2 * scale)
        m.beam(bone, base, a, thick, color=color, pattern="bone")
        m.beam(bone, a, b, thick * 0.9, color=color, pattern="bone")
        m.beam(bone, b, c, thick * 0.8, color=tip or color, pattern="bone")
        # Andouillers : vers l'avant et vers le haut
        for p, d in ((a, (s * 1.0 * scale, 4 * scale, 3.5 * scale)), (b, (s * 1.5 * scale, 5 * scale, 2.5 * scale)), (b, (s * 4 * scale, 3 * scale, -1.5 * scale))):
            m.beam(bone, p, (p[0] + d[0], p[1] + d[1], p[2] + d[2]), thick * 0.75, color=tip or color, pattern="bone")
        if moss:
            m.box(bone, (a[0] - 1.2, a[1] - 1.2, a[2] - 1.2), (a[0] + 1.2, a[1] + 1.2, a[2] + 1.2), moss, "moss")


# ------------------------------------------------------------------ loup


def forest_wolf():
    """Loup des Bois : un grand loup gris-brun, crinière de feuilles et yeux de sève."""
    m = Model("forestWolf")
    fur, dark, light = "#6e6457", "#4a433b", "#b9ab95"
    m.bone("root", None, (0, 13, 0))
    # Corps, poitrail plus clair, échine moussue
    m.box("root", (-4, 8, -11), (4, 16, 6), fur, "fur", {
        "bottom": [("rect", 0, 0, 99, 99, light)], "left": [("rect", 0, 0, 99, 1, dark)], "right": [("rect", 0, 0, 99, 1, dark)]})
    m.box("root", (-5, 7, 3), (5, 17.5, 10), fur, "fur", {"front": [("rect", 2, 3, 6, 99, light)]})
    m.box("root", (-2, 16, -9), (2, 17, 3), MOSS, "moss", {"top": [("pixels", [(1, 2), (2, 6), (1, 9)], SAP_DEEP)]})
    # Crinière de feuilles autour du cou
    for i, (x, y, z, e) in enumerate(((-5.2, 12.5, 6.5, (0, 0, 20)), (5.2, 12.5, 6.5, (0, 0, -20)), (0, 17.8, 6, (20, 0, 0)), (-3.5, 16.5, 7, (10, 0, 25)), (3.5, 16.5, 7, (10, 0, -25)))):
        m.box("root", (x - 1, y - 2, z - 2), (x + 1, y + 2, z + 2), LEAF if i % 2 else LEAF_DARK, "moss",
              {"all": [("px", 1, 1, SAP_DEEP)]}, euler=e)
    m.bone("head", "root", (0, 15, 9))
    wolf_head(m, "head", fur, (0, 12, 15), fur, light, SAP)
    m.bone("tail", "root", (0, 14.5, -11))
    m.push((0, 14.5, -11), (-35, 0, 0))
    m.box("tail", (-1.5, 13, -21), (1.5, 16, -11), fur, "fur")
    m.box("tail", (-1.8, 12.7, -24), (1.8, 16.3, -20), LEAF, "moss", {"back": [("px", 1, 1, SAP_DEEP)]})
    m.pop()
    quadruped_legs(m, 2.5, 7, -8, 9, 9, 3, dark, claws="#d8d0c0", wb=3.2)
    return m


# ------------------------------------------------------------------ ours runique


def runic_bear():
    """Ours Runique : un ours massif, épaulières de pierre gravées de runes qui luisent."""
    m = Model("runicBear")
    fur, dark, muzzle = "#5a3d28", "#3d2a1c", "#8a6a4a"
    stone = "#7d7b74"
    rune = SAP
    m.bone("root", None, (0, 17, 0))
    m.box("root", (-8, 10, -15), (8, 25, 13), fur, "fur", {
        "bottom": [("rect", 0, 0, 99, 99, dark)],
        "left": [("pixels", [(6, 5), (7, 5), (8, 5), (7, 6), (7, 7), (7, 8), (6, 8), (8, 8), (16, 6), (17, 7), (18, 8), (16, 8), (18, 6)], rune)],
        "right": [("pixels", [(9, 5), (10, 5), (11, 5), (10, 6), (10, 7), (10, 8), (9, 8), (11, 8), (19, 6), (20, 7), (21, 8), (19, 8), (21, 6)], rune)]})
    # Bosse d'épaules, gravée d'un sceau
    m.box("root", (-6.5, 25, -2), (6.5, 29, 12), fur, "fur", {
        "top": [("pixels", [(6, 2), (6, 3), (5, 4), (7, 4), (4, 5), (8, 5), (6, 5), (6, 6), (6, 7), (5, 8), (7, 8), (6, 9), (6, 10), (5, 11), (7, 11)], rune)]})
    # Épaulières de pierre runique
    for s in (-1, 1):
        m.box("root", (s * 8 - 1.5, 16, 3), (s * 8 + 1.5, 27, 12), stone, "stone", {
            "sides": [("rect", 0, 0, 99, 1, "#9a978e")],
            "left" if s > 0 else "right": [("pixels", [(3, 3), (4, 3), (5, 3), (4, 4), (4, 5), (3, 6), (5, 6), (4, 7), (4, 8)], rune)]},
              euler=(0, 0, s * -8))
    m.bone("head", "root", (0, 21, 13))
    m.box("head", (-5, 15, 13), (5, 25, 21), fur, "fur", {
        "front": eyes(rune, 1, 3, 0, 2, 1) + [("rect", 4, 0, 2, 1, rune)]})
    m.box("head", (-3, 15, 21), (3, 20, 26), muzzle, "fur", {
        "front": [("rect", 1, 0, 4, 2, "#1d1510")], "top": [("rect", 1, -2, 4, 2, "#1d1510")]})
    for s in (-1, 1):
        m.box("head", (s * 4 - 1.5, 25, 15), (s * 4 + 1.5, 28, 17.5), fur, "fur", {"front": [("rect", 1, 1, 1, 2, dark)]})
    m.bone("tail", "root", (0, 22, -15))
    m.box("tail", (-2, 20, -18), (2, 24, -15), fur, "fur")
    quadruped_legs(m, 5, 8, -10, 11, 11, 5.5, dark, claws="#d9d2c0", wb=6)
    return m


# ------------------------------------------------------------------ grand cerf


def ancient_stag():
    """Grand Cerf Ancien : haut sur pattes, encolure barbue de mousse, bois immenses fleuris."""
    m = Model("ancientStag")
    coat, dark, light = "#8a5a35", "#5c3a20", "#d8c7a8"
    m.bone("root", None, (0, 24, 0))
    m.box("root", (-5, 17, -12), (5, 28, 11), coat, "fur", {
        "bottom": [("rect", 0, 0, 99, 99, light)], "back": [("rect", 2, 2, 6, 7, light)],
        "left": [("pixels", [(x, 2 + (x * 3) % 4) for x in range(3, 20, 3)], light)],
        "right": [("pixels", [(x, 2 + (x * 3) % 4) for x in range(3, 20, 3)], light)]})
    # Mousse le long de l'échine
    m.box("root", (-2.5, 28, -10), (2.5, 29, 8), MOSS, "moss", {"top": [("pixels", [(1, 3), (3, 9), (2, 14)], "#e4e07a")]})
    m.bone("head", "root", (0, 27, 9))
    m.push((0, 26, 9), (28, 0, 0))
    m.box("head", (-3, 24, 6), (3, 38, 12), coat, "fur", {"front": [("rect", 1, 0, 4, 99, light)]})
    # Barbe de mousse sous l'encolure
    m.box("head", (-2, 24, 11.5), (2, 33, 14), MOSS, "moss")
    m.pop()
    hx, hy, hz = 0, 36, 15
    m.box("head", (hx - 3.5, hy, hz - 4), (hx + 3.5, hy + 6, hz + 3), coat, "fur", {
        "front": eyes("#1a1008", 0, 2, 0, 1, 1) + eyes(SAP, 1, 2, 0, 1, 1)})
    m.box("head", (hx - 2.2, hy, hz + 3), (hx + 2.2, hy + 4, hz + 8), light, "fur", {"front": [("rect", 1, 0, 3, 1, "#2a1a10")]})
    for s in (-1, 1):
        m.box("head", (hx + s * 4.5 - 1.5, hy + 4, hz - 2), (hx + s * 4.5 + 1.5, hy + 5.5, hz), coat, "fur", euler=(0, 0, s * -25))
    antlers(m, "head", (hx, hy + 6, hz - 2), 1.3, 1.25, "#cbb48a", tip="#e8dcc0", moss=LEAF, thick=1.6)
    # Fleurs pâles dans les bois
    for s in (-1, 1):
        for (dx, dy, dz) in ((7, 15, -3), (10, 21, -4)):
            m.box("head", (hx + s * dx - 0.8, hy + 6 + dy - 0.8, hz + dz - 0.8), (hx + s * dx + 0.8, hy + 6 + dy + 0.8, hz + dz + 0.8), "#f2f0c8", "flat",
                  {"all": [("px", 0, 0, "#ffd6f0")]})
    m.bone("tail", "root", (0, 26, -12))
    m.box("tail", (-1.5, 23, -13.5), (1.5, 27, -12), light, "fur")
    quadruped_legs(m, 3, 8, -9, 18, 18, 2.6, dark, hoof="#1f1610", wb=3)
    return m


# ------------------------------------------------------------------ faucon


def falcon_wing(m, s, side, root, span, chord, color, under, tip, bone_parent="root", leaf=False):
    """Une aile déployée, à plat : os `wing{side}` à l'épaule, pavé vers ±x, rémiges sombres au bout."""
    x, y, z = root
    name = "wing" + side
    m.bone(name, bone_parent, root)
    inner = (x, y - 0.5, z - chord + 2), (x + s * span * 0.6, y + 0.8, z + 2)
    m.box(name, *inner, color, "feather", {"bottom": [("rect", 0, 0, 99, 99, under)]})
    outer_from = (x + s * span * 0.6, y - 0.4, z - chord + 1)
    outer_to = (x + s * span, y + 0.6, z + 1.5)
    m.box(name, outer_from, outer_to, tip, "feather", {"bottom": [("rect", 0, 0, 99, 99, under)],
                                                     "top": [("rows", list(range(0, 30, 3)), "#1e2228")]})
    # Rémiges : des bandes qui dépassent vers l'arrière
    for k in range(4):
        fx = x + s * (span * 0.55 + k * span * 0.11)
        m.box(name, (fx - 1, y - 0.3, z - chord - 2 + k * 0.5), (fx + 1, y + 0.4, z - chord + 1.5), tip if not leaf else LEAF_DARK, "feather")


def peregrine_falcon():
    """Faucon Pèlerin : dos gris ardoise, poitrail crème barré, moustache noire, grandes ailes."""
    m = Model("peregrineFalcon")
    slate, cream, hood = "#4e5a6a", "#e6dcc4", "#262b33"
    m.bone("root", None, (0, 8, 0))
    m.box("root", (-3.5, 4, -6), (3.5, 10, 6), slate, "feather", {
        "front": [("fn", barred(cream, "#7a6a55"))], "bottom": [("fn", barred(cream, "#7a6a55"))]})
    m.bone("head", "root", (0, 9, 5))
    m.box("head", (-3, 8, 4), (3, 14, 10), hood, "plain", {
        "front": [("rect", 1, 3, 4, 3, cream), ("mirror", ("rect", 0, 1, 2, 2, "#f2c94c")), ("mirror", ("px", 1, 1, "#101010")),
                  ("mirror", ("rect", 1, 3, 1, 3, hood))],
        "left": [("rect", 0, 4, 3, 2, cream)], "right": [("rect", 3, 4, 3, 2, cream)]})
    m.box("head", (-1, 9, 10), (1, 11.5, 12.5), "#f2c94c", "plain", {"front": [("row", 0, "#2b2b2b")], "bottom": [("rect", 0, -1, 2, 1, "#2b2b2b")]})
    m.bone("tail", "root", (0, 7, -6))
    m.box("tail", (-3, 6.4, -13), (3, 7.6, -6), slate, "feather", {"top": [("rows", [1, 3, 5], "#2f3640")], "bottom": [("rows", [1, 3, 5], "#8a8070")]})
    falcon_wing(m, -1, "R", (-3.5, 9, 3), 20, 9, slate, cream, "#3b4552")
    falcon_wing(m, 1, "L", (3.5, 9, 3), 20, 9, slate, cream, "#3b4552")
    for s, side in ((-1, "R"), (1, "L")):
        m.bone("leg" + side, "root", (s * 1.5, 4, 1))
        m.box("leg" + side, (s * 1.5 - 1, 0, 0), (s * 1.5 + 1, 4, 2), "#f2c94c", "plain", {"sides": [("row", -1, "#2b2b2b")]})
    return m


def barred(base, bar):
    """Poitrail barré : un fond crème rayé de petits traits."""
    from boites import rgb

    def fn(img):
        h, w = img.shape[:2]
        for y in range(h):
            for x in range(w):
                if y >= 2 and (y % 2 == 0) and ((x + y // 2) % 3 != 0):
                    img[y, x] = rgb(bar)
                elif y < h:
                    img[y, x] = rgb(base)
    return fn


# ------------------------------------------------------------------ hybrides


def owlbear():
    """Ours-Hibou : corps d'ours, tête et collerette de hibou, griffes en serres."""
    m = Model("owlbear")
    fur, dark, feath, disc = "#6a4c30", "#433021", "#a88a5e", "#e2d2ae"
    m.bone("root", None, (0, 17, 0))
    m.box("root", (-8, 10, -15), (8, 25, 12), fur, "fur", {"bottom": [("rect", 0, 0, 99, 99, dark)]})
    # Poitrail et collerette de plumes
    m.box("root", (-8.5, 12, 6), (8.5, 27, 13), feath, "feather", {})
    m.box("root", (-6.5, 25, -2), (6.5, 28.5, 10), feath, "feather")
    m.bone("head", "root", (0, 22, 13))
    m.box("head", (-6, 17, 12), (6, 29, 20), feath, "feather", {
        "front": [("rect", 1, 2, 10, 9, disc), ("mirror", ("rect", 2, 4, 3, 3, "#f08a24")), ("mirror", ("px", 3, 5, "#120c06")),
                  ("rect", 5, 1, 2, 3, "#5a4026")]})
    # Bec crochu
    m.box("head", (-1.2, 20, 20), (1.2, 24, 22.5), "#3b3530", "plain", {"front": [("row", -1, "#1c1916")]})
    # Aigrettes
    for s in (-1, 1):
        m.box("head", (s * 4.5 - 1, 28, 14), (s * 4.5 + 1, 33, 16), "#5a4026", "feather", euler=(-10, 0, s * -18))
    m.bone("tail", "root", (0, 22, -15))
    m.box("tail", (-3, 20, -19), (3, 24, -15), feath, "feather")
    quadruped_legs(m, 5, 7, -10, 11, 11, 5.5, dark, claws="#2b2522", wb=6)
    # Serres sur les pattes avant
    for x in (-5, 5):
        for k in (-1.8, 0, 1.8):
            m.box("legFR" if x < 0 else "legFL", (x + k - 0.5, 0, 9.5), (x + k + 0.5, 1.2, 11.5), "#ddd4c0", "flat")
    return m


def wendigo():
    """Wendigo : un grand être décharné, voûté, crâne de cerf et bois noircis, griffes interminables."""
    m = Model("wendigo")
    hide, fur, bone, eye = "#4e4740", "#2a2522", "#ddd3bd", "#ff3b2f"
    m.bone("root", None, (0, 26, 0))
    m.bone("legR", "root", (-3, 26, 0))
    m.bone("legL", "root", (3, 26, 0))
    for name, x in (("legR", -3), ("legL", 3)):
        m.box(name, (x - 1.5, 4, -1.5), (x + 1.5, 26, 1.5), hide, "flesh", {"front": [("rect", 0, 8, 3, 3, "#3b352f")]})
        m.box(name, (x - 2, 0, -2), (x + 2, 4, 2.5), "#1a1614", "plain")
    m.box("root", (-4, 24, -2.5), (4, 28, 2.5), fur, "fur")
    # Haut du corps voûté vers l'avant
    m.push((0, 27, 0), (32, 0, 0))
    m.bone("chest", "root", (0, 27, 0))
    m.box("chest", (-4.5, 27, -2.5), (4.5, 41, 2.5), hide, "flesh", {
        "front": [("rows", [3, 5, 7, 9], bone), ("col", 4, bone)], "back": [("col", 4, bone), ("pixels", [(4, k) for k in range(0, 14, 2)], "#f0e8d4")]})
    # Mantelet de poils hirsutes
    m.box("chest", (-6, 36, -3.5), (6, 42, 3.5), fur, "fur", {"sides": [("cut", [(x, 5) for x in range(0, 12, 2)])]})
    m.bone("head", "chest", (0, 42, 1))
    m.box("head", (-3, 42, -1.5), (3, 48, 4.5), bone, "bone", {
        "front": [("mirror", ("rect", 0, 2, 2, 2, "#120c0a")), ("mirror", ("px", 1, 3, eye))]})
    m.box("head", (-2, 42, 4.5), (2, 46, 10), bone, "bone", {"front": [("rect", 0, 2, 4, 2, "#2a2020")], "bottom": [("pixels", [(0, 1), (3, 1), (0, 3), (3, 3)], "#f8f4ea")]})
    antlers(m, "head", (0, 48, 1.5), 1.4, 1.1, "#2d2621", tip="#4a3f36", thick=1.5)
    for name, s in (("armR", -1), ("armL", 1)):
        m.bone(name, "chest", (s * 5.5, 39, 0))
        m.box(name, (s * 5.5 - 1.75, 16, -1.75), (s * 5.5 + 1.75, 40, 1.75), hide, "flesh", {"sides": [("rect", 0, 0, 99, 5, fur)]})
        for k in (-1, 0, 1):
            m.beam(name, (s * 5.5 + k * 1.0, 16.5, 0.5), (s * 5.5 + k * 1.3, 10, 2.5), 0.6, color=bone, pattern="bone")
    m.pop()
    return m


def sylvan_griffin():
    """Griffon Sylvestre : avant d'aigle à tête blanche, arrière de fauve, ailes feuillues."""
    m = Model("sylvanGriffin")
    tawny, dark, white, wing = "#a67c46", "#6e5230", "#eee8d8", "#5c6b3a"
    m.bone("root", None, (0, 17, 0))
    m.box("root", (-6, 11, -14), (6, 22, 4), tawny, "fur", {"bottom": [("rect", 0, 0, 99, 99, "#d2b07a")]})
    m.box("root", (-6.5, 12, 3), (6.5, 24, 11), white, "feather")
    m.bone("head", "root", (0, 22, 10))
    m.box("head", (-3.5, 21, 9), (3.5, 28, 16), white, "feather", {
        "front": [("mirror", ("rect", 0, 2, 2, 1, "#f2c94c")), ("mirror", ("px", 1, 2, "#101010"))]})
    m.box("head", (-1.5, 22, 16), (1.5, 26, 19.5), "#e0a82e", "plain", {"front": [("row", -1, "#4a3812")], "bottom": [("row", -1, "#4a3812")]})
    # Crête de feuilles
    for k, z in enumerate((10, 12.5, 15)):
        m.box("head", (-0.6, 28, z - 1.5), (0.6, 30.5 - k * 0.5, z + 1.5), LEAF if k % 2 else LEAF_DARK, "moss", euler=(-20, 0, 0))
    m.bone("tail", "root", (0, 20, -14))
    m.push((0, 20, -14), (-40, 0, 0))
    m.box("tail", (-1, 19, -26), (1, 21, -14), tawny, "fur")
    m.box("tail", (-2, 18, -30), (2, 22, -25), LEAF, "moss")
    m.pop()
    quadruped_legs(m, 3.5, 7, -10, 11, 11, 3, dark, wb=4)
    for x, name in ((-3.5, "legFR"), (3.5, "legFL")):
        m.box(name, (x - 1.7, 0, 5.5), (x + 1.7, 2.5, 8.5), "#e0a82e", "plain", {"front": [("pixels", [(0, -1), (2, -1)], "#2b2522")]})
    # Ailes repliées à demi, dressées vers l'arrière
    for s, side in ((-1, "R"), (1, "L")):
        m.push((s * 6, 22, 4), (0, s * 50, s * 22))
        falcon_wing(m, s, side, (s * 6, 22, 4), 22, 11, wing, "#c8b88a", "#4a5a2c", leaf=True)
        m.pop()
    return m


def void_chimera():
    """Chimère du Néant (Avatar du Néant) : un fauve colossal à crinière de cristaux d'Étherium, bois
    de cerf, ailes de faucon et queue de cristaux."""
    m = Model("voidChimera")
    hide, dark, mane = "#2a2238", "#18121f", "#3d2f55"
    cyan, violet = "#5ff3ff", "#b46bff"
    m.bone("root", None, (0, 30, 0))
    m.box("root", (-12, 20, -26), (12, 42, 18), hide, "fur", {
        "bottom": [("rect", 0, 0, 99, 99, dark)],
        "left": [("pixels", [(x, 4 + (x * 7) % 12) for x in range(2, 44, 4)], cyan)],
        "right": [("pixels", [(x, 4 + (x * 5) % 12) for x in range(2, 44, 4)], violet)]}, res=0.75)
    # Crinière et cristaux
    m.box("root", (-14, 18, 8), (14, 46, 22), mane, "fur", {}, res=0.75)
    m.bone("head", "root", (0, 40, 20))
    m.box("head", (-7, 32, 20), (7, 46, 34), hide, "fur", {
        "front": [("mirror", ("rect", 1, 4, 4, 2, cyan)), ("mirror", ("px", 3, 4, "#ffffff")), ("rect", 4, 9, 6, 2, dark)]})
    m.box("head", (-4.5, 32, 34), (4.5, 39, 41), mane, "fur", {"front": [("rect", 2, 0, 5, 2, "#0d0a12")],
                                                              "bottom": [("pixels", [(0, 0), (8, 0), (2, 0), (6, 0)], "#f0ecf8")]})
    antlers(m, "head", (0, 46, 26), 1.5, 1.4, "#3a3050", tip=cyan, thick=2.4, scale=1.6)
    crystals = [(-9, 46, 14, (10, 0, 20)), (9, 46, 14, (10, 0, -20)), (0, 44, 2, (-15, 0, 0)), (-6, 43, -8, (-10, 0, 25)),
                (6, 43, -8, (-10, 0, -25)), (0, 43, -18, (-20, 0, 0)), (-12, 40, 18, (0, 0, 40)), (12, 40, 18, (0, 0, -40))]
    for k, (x, y, z, e) in enumerate(crystals):
        c = cyan if k % 2 == 0 else violet
        m.box("root", (x - 1.6, y - 3, z - 1.6), (x + 1.6, y + 6 + (k % 3) * 2, z + 1.6), c, "crystal", euler=e)
    m.bone("tail", "root", (0, 38, -26))
    m.push((0, 38, -26), (-30, 0, 0))
    m.box("tail", (-2.5, 36, -44), (2.5, 41, -26), hide, "fur")
    for k in range(3):
        m.box("tail", (-1.5, 40, -30 - k * 5), (1.5, 45 - k, -27 - k * 5), violet if k % 2 else cyan, "crystal", euler=(-20, 0, 0))
    m.box("tail", (-2, 35, -50), (2, 42, -44), cyan, "crystal")
    m.pop()
    quadruped_legs(m, 7.5, 10, -18, 22, 22, 7, dark, claws=cyan, wb=8)
    for s, side in ((-1, "R"), (1, "L")):
        m.push((s * 10, 42, 0), (0, s * 45, s * 25))
        falcon_wing(m, s, side, (s * 10, 42, 0), 40, 16, mane, "#5a4a7a", "#21182f")
        m.pop()
    # Liseré cristallin des ailes
    return m


ALL = [forest_wolf, runic_bear, ancient_stag, peregrine_falcon, owlbear, wendigo, sylvan_griffin, void_chimera]
