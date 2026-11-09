"""Les créatures du Nécromancien (golem de chair, béhémoth d'os) et son mur d'os."""
from boites import Model

BONE = "#ddd3bd"
BONE_DARK = "#b3a688"
SOUL = "#7dff9a"


def flesh_golem():
    """Golem de Chair : une masse de chairs recousues, bras inégaux, ferrures et os qui percent."""
    m = Model("fleshGolem")
    flesh, pale, bruise, stitch, iron = "#b77d73", "#d2a08f", "#7b4f5c", "#2a1a1a", "#4a4a4f"
    m.bone("root", None, (0, 16, 0))
    for name, x in (("legR", -5), ("legL", 5)):
        m.bone(name, "root", (x, 16, 0))
        m.box(name, (x - 3.5, 0, -3.5), (x + 3.5, 16, 3.5), flesh, "flesh", {
            "front": [("col", 3, stitch), ("pixels", [(2, k) for k in range(1, 16, 2)] + [(4, k) for k in range(1, 16, 2)], stitch)],
            "bottom": [("rect", 0, 0, 99, 99, bruise)]})
        m.box(name, (x - 4, 6, -4), (x + 4, 8, 4), iron, "metal")
    m.bone("torso", "root", (0, 18, 0))
    m.push((0, 18, 0), (12, 0, 0))
    m.box("torso", (-10, 15, -6), (10, 37, 7), flesh, "flesh", {
        "front": [("rect", 3, 4, 7, 6, pale), ("rect", 12, 12, 5, 7, bruise),
                  ("row", 11, stitch), ("pixels", [(k, 10) for k in range(0, 20, 2)] + [(k, 12) for k in range(0, 20, 2)], stitch),
                  ("col", 10, stitch), ("pixels", [(9, k) for k in range(0, 22, 2)] + [(11, k) for k in range(0, 22, 2)], stitch)],
        "back": [("col", 6, stitch), ("pixels", [(5, k) for k in range(0, 22, 2)] + [(7, k) for k in range(0, 22, 2)], stitch)]}, res=1.0)
    # Os qui percent le dos, ceinture de fer
    for k, (x, y) in enumerate(((-5, 32), (0, 34), (5, 31), (-2, 26), (4, 24))):
        m.beam("torso", (x, y, -6), (x + (k % 2) * 1.5, y + 3, -10), 1.2, color=BONE, pattern="bone")
    m.box("torso", (-10.5, 18, -6.5), (10.5, 20.5, 7.5), iron, "rivets")
    m.bone("head", "torso", (0, 36, 3))
    m.box("head", (-4, 35, -1), (4, 42, 6), pale, "flesh", {
        "front": [("rect", 1, 2, 2, 1, "#111"), ("rect", 5, 2, 1, 1, "#e8e04a"), ("rect", 1, 5, 6, 1, stitch),
                  ("pixels", [(1, 4), (3, 4), (5, 4), (2, 6), (4, 6), (6, 6)], stitch)],
        "top": [("col", 4, stitch)]})
    # Bras droit énorme, bras gauche greffé plus mince
    m.bone("armR", "torso", (-11, 34, 0))
    m.box("armR", (-18, 12, -5), (-10, 37, 5), flesh, "flesh", {
        "sides": [("row", 12, stitch), ("pixels", [(k, 11) for k in range(0, 10, 2)] + [(k, 13) for k in range(0, 10, 2)], stitch)]})
    m.box("armR", (-19, 6, -6), (-9, 13, 6), bruise, "flesh", {"front": [("pixels", [(1, -1), (4, -1), (7, -1)], BONE)]})
    m.box("armR", (-18.5, 28, -5.5), (-9.5, 30, 5.5), iron, "metal")
    m.bone("armL", "torso", (10, 34, 0))
    m.box("armL", (10, 16, -3), (15, 37, 3), pale, "flesh", {"sides": [("row", 9, stitch), ("pixels", [(k, 8) for k in range(0, 6, 2)] + [(k, 10) for k in range(0, 6, 2)], stitch)]})
    m.box("armL", (9.5, 11, -3.5), (15.5, 16, 3.5), flesh, "flesh")
    for k in (-1.5, 0, 1.5):
        m.beam("armL", (12.5 + k, 11.5, 2), (12.5 + k * 1.3, 8, 4), 0.8, color=BONE, pattern="bone")
    m.pop()
    return m


def bone_behemoth():
    """Béhémoth d'Os : un quadrupède colossal fait d'os assemblés, une âme verte dans la cage."""
    m = Model("boneBehemoth")
    m.bone("root", None, (0, 30, 0))
    # Âme verte au cœur de la cage thoracique
    m.box("root", (-5, 24, -8), (5, 34, 6), SOUL, "glow", {"all": [("frame", "#c8ffd6")]})
    # Échine : vertèbres et épines
    for k in range(9):
        z = 16 - k * 5
        m.box("root", (-2.5, 40, z - 2), (2.5, 44, z + 2), BONE, "bone")
        m.beam("root", (0, 44, z), (0, 50 - abs(k - 3) * 0.8, z - 2.5), 1.6, color=BONE_DARK, pattern="bone")
    # Côtes : des arcs de chaque côté
    for k in range(6):
        z = 12 - k * 4.5
        for s in (-1, 1):
            m.beam("root", (s * 2, 41, z), (s * 11, 36, z), 1.6, color=BONE, pattern="bone")
            m.beam("root", (s * 11, 36, z), (s * 10, 24, z - 1), 1.6, color=BONE, pattern="bone")
            m.beam("root", (s * 10, 24, z - 1), (s * 4, 20, z - 1), 1.4, color=BONE_DARK, pattern="bone")
    # Bassin et omoplates
    m.box("root", (-10, 28, -30), (10, 38, -20), BONE, "bone", {"back": [("rect", 4, 3, 3, 3, "#2b2620"), ("rect", 13, 3, 3, 3, "#2b2620")]})
    for s in (-1, 1):
        m.box("root", (s * 12 - 2, 30, 10), (s * 12 + 2, 42, 18), BONE, "bone", euler=(0, 0, s * -12))
    m.bone("head", "root", (0, 40, 18))
    m.box("head", (-8, 32, 18), (8, 46, 32), BONE, "bone", {
        "front": [("mirror", ("rect", 2, 3, 4, 4, "#14110e")), ("mirror", ("rect", 3, 4, 2, 2, SOUL)), ("rect", 7, 8, 2, 3, "#14110e")]})
    m.box("head", (-6, 30, 30), (6, 38, 40), BONE, "bone", {"front": [("row", 3, "#2b2620")]})
    for s in (-1, 1):
        for k in range(5):
            m.box("head", (s * (5 - k * 2.2) - 0.7, 28, 37.5 - k * 0.1), (s * (5 - k * 2.2) + 0.7, 31, 39.5), "#f2ebdc", "flat")
    # Cornes recourbées
    for s in (-1, 1):
        m.beam("head", (s * 7, 44, 24), (s * 15, 50, 22), 3, color=BONE_DARK, pattern="bone")
        m.beam("head", (s * 15, 50, 22), (s * 18, 52, 30), 2.4, color=BONE_DARK, pattern="bone")
        m.beam("head", (s * 18, 52, 30), (s * 17, 49, 35), 1.6, color="#8f846c", pattern="bone")
    # Pattes : os longs et larges sabots
    for name, x, z in (("legFR", -10, 12), ("legFL", 10, 12), ("legBR", -10, -25), ("legBL", 10, -25)):
        m.bone(name, "root", (x, 32, z))
        m.beam(name, (x, 32, z), (x, 16, z - 2), 4.5, color=BONE, pattern="bone")
        m.box(name, (x - 3, 14, z - 4.5), (x + 3, 18, z + 0.5), BONE_DARK, "bone")
        m.beam(name, (x, 16, z - 2), (x, 3, z), 3.5, color=BONE, pattern="bone")
        m.box(name, (x - 4, 0, z - 3), (x + 4, 4, z + 4), "#8f846c", "bone", {"front": [("pixels", [(1, -1), (4, -1), (6, -1)], "#5a5244")]})
    m.bone("tail", "root", (0, 38, -30))
    for k in range(6):
        m.box("tail", (-1.5 + k * 0.15, 37 - k * 2.4, -32 - k * 3.2), (1.5 - k * 0.15, 40 - k * 2.4, -29 - k * 3.2), BONE if k % 2 else BONE_DARK, "bone")
    return m


def skeleton_servant():
    """Serviteur Squelette : rien que les os, à la taille d'un héros (1,8 bloc). Crâne aux orbites
    vertes, cage thoracique ajourée, os longs fins, épée courte rouillée dans la main droite.
    Os : `root` (bassin), `chest`, `head`, `armR`, `armL`, `legR`, `legL`."""
    m = Model("skeletonServant")
    bone, dark, rust = BONE, BONE_DARK, "#8a5a32"
    m.bone("root", None, (0, 14, 0))
    # Bassin
    m.box("root", (-3.5, 12.5, -1.5), (3.5, 15, 1.5), bone, "bone", {"front": [("rect", 2, 1, 3, 1, "#2a2420")]})
    for name, x in (("legR", -2), ("legL", 2)):
        m.bone(name, "root", (x, 13, 0))
        m.beam(name, (x, 13, 0), (x, 7, 0.3), 1.3, color=bone, pattern="bone")      # fémur
        m.box(name, (x - 1, 6, -0.7), (x + 1, 7.5, 1.2), dark, "bone")               # genou
        m.beam(name, (x, 6.5, 0.3), (x, 1, 0), 1.1, color=bone, pattern="bone")      # tibia
        m.box(name, (x - 1.1, 0, -1), (x + 1.1, 1, 2.2), bone, "bone")                # pied
    m.bone("chest", "root", (0, 15, 0))
    # Colonne et cage thoracique : quatre côtes en anneau, rien dedans
    m.beam("chest", (0, 14.5, -1), (0, 25, -1), 1.2, color=dark, pattern="bone")
    for k, y in enumerate((17.5, 19.5, 21.5, 23.2)):
        w = 3.6 - abs(k - 2) * 0.4
        m.box("chest", (-w, y, 1.6), (w, y + 0.9, 2.3), bone, "bone")                  # devant
        m.box("chest", (-w, y, -1.6), (w, y + 0.9, -0.9), bone, "bone")                # dos
        for sx in (-1, 1):
            m.box("chest", (sx * w - 0.35, y, -1.6), (sx * w + 0.35, y + 0.9, 2.3), bone, "bone")
    m.box("chest", (-0.5, 17.5, 1.4), (0.5, 24, 2.4), dark, "bone")                     # sternum
    # Clavicules
    m.box("chest", (-5, 24.2, -0.8), (5, 25.1, 0.6), bone, "bone")
    m.bone("head", "chest", (0, 25.5, 0))
    m.beam("head", (0, 24.8, -0.6), (0, 26.2, -0.4), 0.9, color=dark, pattern="bone")  # cou
    m.box("head", (-3, 26, -2.6), (3, 31.5, 3), bone, "bone", {
        "front": [("rect", 0, 1, 2, 2, "#120f0c"), ("rect", 4, 1, 2, 2, "#120f0c"), ("px", 1, 2, SOUL), ("px", 4, 2, SOUL),
                  ("pixels", [(2, 3), (3, 3)], "#120f0c")], "back": [("px", 2, 1, dark)]})
    m.box("head", (-2.4, 24.6, -1.2), (2.4, 26, 2.8), bone, "bone", {
        "front": [("row", 0, "#2a2420"), ("pixels", [(0, 0), (2, 0), (4, 0)], "#f2ebdc")]})                     # mâchoire
    for name, s in (("armR", -1), ("armL", 1)):
        x = s * 4.8
        m.bone(name, "chest", (x, 24, 0))
        m.box(name, (x - 1, 23.4, -1), (x + 1, 25.2, 1), dark, "bone")                 # épaule
        m.beam(name, (x, 23.6, 0), (x + s * 0.3, 18.2, 0), 1.1, color=bone, pattern="bone")   # humérus
        m.beam(name, (x + s * 0.3, 18.4, 0), (x + s * 0.3, 13.6, 0.4), 0.9, color=bone, pattern="bone")  # avant-bras
        m.box(name, (x + s * 0.3 - 0.8, 12, -0.4), (x + s * 0.3 + 0.8, 13.8, 1.4), bone, "bone")      # main
    # Épée courte rouillée, tenue la pointe en avant
    hx = -4.5
    m.box("armR", (hx - 0.5, 12.2, -1.8), (hx + 0.5, 13.4, 1.6), "#4a3220", "wood")
    m.box("armR", (hx - 1.8, 12.1, 1.6), (hx + 1.8, 13.5, 2.4), rust, "metal")
    m.box("armR", (hx - 0.6, 12.3, 2.4), (hx + 0.6, 13.3, 11), "#a9a294", "metal", {"top": [("pixels", [(0, 2), (0, 5), (0, 7)], rust)]})
    # Lambeau de pagne
    m.box("root", (-3.6, 10.5, 1.5), (3.6, 13, 1.9), "#5a3e24", "plain", {"front": [("cut", [(0, -1), (3, -1), (4, -2), (6, -1)])]})
    return m


def bone_wall():
    """Mur d'Ossements : des os pointus surgis du sol, entrelacés de côtes, crânes à la base.
    5 blocs de long, 2,5 de haut. L'animation `rise` le fait jaillir du sol."""
    m = Model("boneWall", seed=7)
    m.bone("root", None, (0, 0, 0))
    rng = m.rng
    for k in range(15):
        x = -38 + k * 5.4 + rng.uniform(-1, 1)
        h = 26 + rng.uniform(0, 14) * (1 - abs(k - 7) / 9)
        lean = rng.uniform(-6, 6)
        tilt = rng.uniform(-3, 5)
        w = rng.uniform(3, 4.5)
        m.beam("root", (x, 0, rng.uniform(-2, 2)), (x + lean * 0.5, h * 0.65, tilt * 0.5), w, color="#d8ccb2", pattern="bone")
        m.beam("root", (x + lean * 0.5, h * 0.65, tilt * 0.5), (x + lean, h, tilt), w * 0.55, color="#ece4d2", pattern="bone")
    for y in (8, 16):
        for k in range(5):
            x0 = -38 + k * 16
            m.beam("root", (x0, y + rng.uniform(-2, 2), 2.5), (x0 + 16, y + rng.uniform(-2, 2), 2.5), 1.6, color="#c4b796", pattern="bone")
    for x in (-30, -6, 20):
        m.box("root", (x - 3, 0, 2), (x + 3, 6, 8), "#e6dcc6", "bone", {
            "front": [("mirror", ("rect", 1, 2, 1, 1, "#1a1612")), ("px", 2, 4, "#1a1612"), ("px", 3, 4, "#1a1612"), ("row", 5, "#d0c4a8")]})
    m.box("root", (-42, 0, -6), (42, 1.5, 6), "#5a4a38", "stone", res=0.5)
    m.clip("rise", 0.6, [("root", "translation", [(0, [0, -44, 0]), (0.45, [0, 3, 0]), (0.6, [0, 0, 0])])])
    return m


ALL = [flesh_golem, bone_behemoth, skeleton_servant, bone_wall]
