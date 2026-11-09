"""Les machines de l'Ingénieur : tourelles, pièges, mine, barricade et forteresse de l'ultime ; et
le mur de pierre du Sorcier (aussi celui du mur de magma, teinté de rouge).

Chaque modèle porte ses animations glTF (`clip`) : `fire` (un tir), `snap` (le piège claque),
`spin`, `blink`, `unfold` (la barricade se déplie), `rise` (le mur ou la forteresse sort du sol).
"""
from boites import Model

IRON = "#55575c"
IRON_DARK = "#36383c"
BRASS = "#b88a3e"
COPPER = "#c06a3a"
WOOD = "#8a6238"
WOOD_DARK = "#5e4024"
ETHER = "#5ff3ff"


def tripod(m, bone, top, spread, color):
    for k, (dx, dz) in enumerate(((0, 1), (-0.87, -0.5), (0.87, -0.5))):
        m.beam(bone, (0, top, 0), (dx * spread, 0, dz * spread), 1.6, color=color, pattern="wood")
        m.box(bone, (dx * spread - 1.2, 0, dz * spread - 1.2), (dx * spread + 1.2, 1, dz * spread + 1.2), IRON_DARK, "metal")


def ballistic_turret():
    """Tourelle Balistique : un trépied de bois et une baliste à carreaux qui pivote (`head`)."""
    m = Model("ballisticTurret")
    m.bone("root", None, (0, 0, 0))
    tripod(m, "root", 10, 7, WOOD)
    m.box("root", (-2, 9, -2), (2, 12, 2), IRON, "rivets")
    m.bone("head", "root", (0, 12, 0))
    # Fût, arc et corde, carreau engagé
    m.box("head", (-1.5, 12, -7), (1.5, 15, 8), WOOD_DARK, "wood", {"top": [("col", 1, "#3a2814")]})
    m.box("head", (-2, 11.5, -8), (2, 15.5, -5), IRON, "rivets")
    for s in (-1, 1):
        m.beam("head", (s * 1.5, 14, 6), (s * 9, 14, 2), 1.4, color=WOOD, pattern="wood")
        m.beam("head", (s * 9, 14, 2), (s * 0.4, 14, -3), 0.3, color="#e8e0c8", pattern="flat")
    m.box("head", (-0.5, 15, -3), (0.5, 16, 10), "#7a6a55", "wood", {"front": [("rect", 0, 0, 1, 1, "#c9c9c9")]})
    m.box("head", (-0.9, 14.8, 10), (0.9, 16.2, 11.5), "#b8bcc2", "metal")
    m.box("head", (-0.3, 16, -3), (0.3, 17, -1), "#d64a3a", "flat")
    m.clip("fire", 0.5, [("head", "translation", [(0, [0, 0, 0]), (0.06, [0, 0, -1.5]), (0.5, [0, 0, 0])]),
                         ("head", "rotation", [(0, [0, 0, 0]), (0.06, [-6, 0, 0]), (0.5, [0, 0, 0])])])
    return m


def flame_turret():
    """Tourelle Incendiaire : un petit canon trapu, réservoir rouge, flamme pilote à la buse."""
    m = Model("flameTurret")
    m.bone("root", None, (0, 0, 0))
    m.box("root", (-5, 0, -5), (5, 4, 5), IRON_DARK, "rivets")
    m.box("root", (-5, 0, -5), (5, 4, 5), IRON_DARK, "rivets", euler=(0, 45, 0))
    m.box("root", (-3, 4, -3), (3, 7, 3), IRON, "metal")
    m.bone("head", "root", (0, 8, 0))
    m.box("head", (-3.5, 7, -4), (3.5, 12, 4), IRON, "rivets", {"front": [("rect", 1, 1, 5, 1, "#e07a2a")]})
    m.box("head", (-1.5, 8, 4), (1.5, 11, 12), "#3d3f44", "metal", {"top": [("rows", [2, 5], BRASS)]})
    m.box("head", (-2, 7.5, 11), (2, 11.5, 13), BRASS, "metal", {"front": [("rect", 1, 1, 2, 2, "#1a1410")]})
    m.box("head", (-0.6, 9.3, 13), (0.6, 10.5, 14), "#ffb347", "glow", {"all": [("px", 0, 0, "#fff0a0")]})
    # Réservoir de combustible au dos
    m.box("head", (-2.5, 8, -9), (2.5, 14, -4), "#b03a2a", "metal", {"back": [("row", 1, "#e0d0a0"), ("rect", 1, 3, 3, 1, "#1a1410")]})
    m.beam("head", (0, 13.5, -6), (0, 12.5, 2), 0.8, color=BRASS, pattern="metal")
    m.clip("fire", 0.3, [("head", "translation", [(0, [0, 0, 0]), (0.05, [0, 0.3, -0.6]), (0.15, [0, -0.2, 0.2]), (0.3, [0, 0, 0])])])
    return m


def jaw_trap():
    """Piège à Mâchoire : deux mâchoires dentées ouvertes à plat, ressorts et plaque de pression.
    `snap` les referme (os `jawR`, `jawL`)."""
    m = Model("jawTrap")
    m.bone("root", None, (0, 0, 0))
    m.box("root", (-1.5, 0, -6), (1.5, 1, 6), IRON_DARK, "metal")
    m.box("root", (-2.5, 0.5, -2.5), (2.5, 1.2, 2.5), "#8a7a5a", "metal", {"top": [("frame", "#5a4a3a")]})
    for s in (-1, 1):
        m.box("root", (s * 2 - 1, 0, -7), (s * 2 + 1, 1.6, -5), "#6e7076", "metal")
        m.box("root", (s * 2 - 1, 0, 5), (s * 2 + 1, 1.6, 7), "#6e7076", "metal")
    for s, name in ((-1, "jawR"), (1, "jawL")):
        m.bone(name, "root", (s * 1.5, 0.8, 0))
        # Arc de la mâchoire, à plat vers l'extérieur
        m.box(name, (s * 1.5, 0.3, -6), (s * 7.5, 1.3, -5), "#6e7076", "metal")
        m.box(name, (s * 1.5, 0.3, 5), (s * 7.5, 1.3, 6), "#6e7076", "metal")
        m.box(name, (s * 7.5 - (1 if s > 0 else 0), 0.3, -6), (s * 7.5 + (0 if s > 0 else 1), 1.3, 6), "#6e7076", "metal")
        for k in range(5):
            z = -4 + k * 2
            m.box(name, (s * 6.5 - 0.5, 1.3, z - 0.5), (s * 6.5 + 0.5, 2.6, z + 0.5), "#d8dade", "flat")
    m.box("root", (-8, 0, -8), (8, 0.3, 8), "#3a2e22", "plain", faces=("top",), alpha_faces=True,
          decals={"top": [("fn", ring_mask)]})
    m.clip("snap", 0.25, [("jawR", "rotation", [(0, [0, 0, 0]), (0.12, [0, 0, -95]), (0.25, [0, 0, -88])]),
                          ("jawL", "rotation", [(0, [0, 0, 0]), (0.12, [0, 0, 95]), (0.25, [0, 0, 88])])])
    return m


def ring_mask(img):
    """Une tache de terre retournée, ronde : les coins de la case sont découpés."""
    h, w = img.shape[:2]
    for y in range(h):
        for x in range(w):
            dx, dy = (x + 0.5 - w / 2) / (w / 2), (y + 0.5 - h / 2) / (h / 2)
            if dx * dx + dy * dy > 1:
                img[y, x] = (0, 0, 0, 0)


def magnetic_trap():
    """Piège Magnétique : disque de cuivre, bobines et cœur orange ; l'anneau (`ring`) tourne."""
    m = Model("magneticTrap")
    m.bone("root", None, (0, 0, 0))
    for a in (0, 45):
        m.box("root", (-7, 0, -7), (7, 1, 7), COPPER, "metal", euler=(0, a, 0))
    m.box("root", (-2, 1, -2), (2, 5, 2), "#3d3f44", "metal", {"sides": [("rows", [0, 2], COPPER)]})
    m.box("root", (-1, 5, -1), (1, 6.5, 1), "#ff9a3c", "glow", {"all": [("px", 0, 0, "#fff0b0")]})
    m.bone("ring", "root", (0, 1, 0))
    for k in range(4):
        a = k * 90
        m.push((0, 0, 0), (0, a, 0))
        m.box("ring", (-1.5, 1, 4), (1.5, 3.5, 6.5), "#3d3f44", "metal", {"sides": [("rows", [0, 1, 2], COPPER)]})
        m.box("ring", (-0.5, 1, 1.5), (0.5, 1.6, 4), COPPER, "metal")
        m.pop()
    m.clip("spin", 1.2, [("ring", "rotation", [(0, [0, 0, 0]), (0.4, [0, 120, 0]), (0.8, [0, 240, 0]), (1.2, [0, 360, 0])])])
    return m


def shrapnel_mine():
    """Mine à Éclats : un petit dôme d'acier hérissé de pointes, voyant rouge (`light`) qui clignote."""
    m = Model("shrapnelMine")
    m.bone("root", None, (0, 0, 0))
    for a in (0, 45):
        m.box("root", (-3.5, 0, -3.5), (3.5, 1.5, 3.5), IRON_DARK, "rivets", euler=(0, a, 0))
    m.box("root", (-2.5, 1.5, -2.5), (2.5, 3, 2.5), IRON, "metal")
    for k in range(8):
        m.push((0, 0, 0), (0, k * 45, 0))
        m.beam("root", (0, 1.8, 2.4), (0, 2.6, 4.2), 0.6, color="#c8ccd2", pattern="flat")
        m.pop()
    m.bone("light", "root", (0, 3, 0))
    m.box("light", (-0.7, 3, -0.7), (0.7, 4, 0.7), "#ff3030", "glow", {"top": [("px", 0, 0, "#ffc0c0")]})
    m.clip("blink", 1.0, [("light", "scale", [(0, [1, 1, 1]), (0.45, [1, 1, 1]), (0.5, [1.6, 1.4, 1.6]), (0.6, [1, 1, 1]), (1.0, [1, 1, 1])])])
    return m


def portable_barricade():
    """Barricade Portative dépliée : trois panneaux de planches cerclés de fer, pointes au sommet.
    3 blocs de large. `unfold` déplie les volets (`panelR`, `panelL`)."""
    m = Model("portableBarricade")
    m.bone("root", None, (0, 0, 0))

    def panel(bone, x0, x1):
        m.box(bone, (x0, 1, -1), (x1, 20, 1), WOOD, "planks", {"front": [("rows", [0, 6, 12], IRON_DARK)], "back": [("rows", [0, 6, 12], IRON_DARK)]})
        for y in (3, 10, 17):
            m.box(bone, (x0, y, -1.3), (x1, y + 1.2, 1.3), IRON, "rivets")
        for k in range(int(abs(x1 - x0) // 4)):
            x = min(x0, x1) + 2 + k * 4
            m.beam(bone, (x, 20, 0), (x, 24, 1), 1.0, color="#c8ccd2", pattern="metal")

    panel("root", -8, 8)
    m.box("root", (-8, 0, -4), (8, 1, 4), IRON_DARK, "metal")
    m.beam("root", (0, 12, -1), (0, 0, -7), 1.4, color=IRON, pattern="metal")
    for s, name in ((-1, "panelR"), (1, "panelL")):
        m.bone(name, "root", (s * 8, 0, 0))
        m.push((s * 8, 0, 0), (0, s * -30, 0))
        panel(name, s * 8, s * 24)
        m.beam(name, (s * 18, 12, -1), (s * 18, 0, -7), 1.4, color=IRON, pattern="metal")
        m.pop()
    m.clip("unfold", 0.7, [("panelR", "rotation", [(0, [0, -150, 0]), (0.5, [0, 8, 0]), (0.7, [0, 0, 0])]),
                           ("panelL", "rotation", [(0, [0, 150, 0]), (0.5, [0, -8, 0]), (0.7, [0, 0, 0])])])
    return m


def etherium_bastion():
    """Bastion d'Étherium : une forteresse carrée autour de l'Ingénieur (7 blocs de côté), quatre
    tours coiffées de cristaux et de canons (`cannonNE`…), une porte. `rise` la fait sortir du sol."""
    m = Model("etheriumBastion")
    stone, dark = "#6a6f78", "#474b53"
    m.bone("root", None, (0, 0, 0))
    half = 56
    # Murs : un pan par côté, la porte au sud (vers −z)... percée devant (+z)
    for k, (axis, sign) in enumerate((("z", 1), ("z", -1), ("x", 1), ("x", -1))):
        if axis == "z":
            if sign > 0:
                for x0, x1 in ((-half + 8, -8), (8, half - 8)):
                    m.box("root", (x0, 0, half - 6), (x1, 22, half - 1), stone, "stone", {"sides": [("row", 0, dark)]}, res=0.5)
                m.box("root", (-8, 16, half - 6), (8, 22, half - 1), stone, "stone", res=0.5)
            else:
                m.box("root", (-half + 8, 0, -half + 1), (half - 8, 22, -half + 6), stone, "stone", {"sides": [("row", 0, dark)]}, res=0.5)
        else:
            x0 = sign * (half - 6)
            m.box("root", (min(x0, sign * (half - 1)), 0, -half + 8), (max(x0, sign * (half - 1)), 22, half - 8), stone, "stone", {"sides": [("row", 0, dark)]}, res=0.5)
    # Créneaux et liseré d'Étherium
    for k in range(-5, 6):
        for z in (half - 3.5, -half + 3.5):
            if z > 0 and abs(k * 9) < 9:
                continue
            m.box("root", (k * 9 - 2.5, 22, z - 2.5), (k * 9 + 2.5, 26, z + 2.5), dark, "stone")
        for x in (half - 3.5, -half + 3.5):
            m.box("root", (x - 2.5, 22, k * 9 - 2.5), (x + 2.5, 26, k * 9 + 2.5), dark, "stone")
    for z in (half - 1, -half + 1):
        m.box("root", (-half + 8, 10, z - 0.3), (half - 8, 11, z + 0.3), ETHER, "glow")
    # Tours d'angle
    for name, sx, sz in (("cannonNE", 1, 1), ("cannonNW", -1, 1), ("cannonSE", 1, -1), ("cannonSW", -1, -1)):
        cx, cz = sx * (half - 6), sz * (half - 6)
        m.box("root", (cx - 9, 0, cz - 9), (cx + 9, 32, cz + 9), "#5a5f68", "stone", {"sides": [("rect", 7, 6, 4, 6, "#1d2026"), ("row", 0, dark)]}, res=0.5)
        m.box("root", (cx - 10, 32, cz - 10), (cx + 10, 35, cz + 10), dark, "stone", res=0.5)
        for a in (0, 45):
            m.box("root", (cx - 4, 35, cz - 4), (cx + 4, 56, cz + 4), ETHER, "crystal", euler=(0, a, 0))
        m.bone(name, "root", (cx, 36, cz))
        m.box(name, (cx - 3, 35, cz - 3), (cx + 3, 39, cz + 3), IRON, "rivets")
        m.push((cx, 37, cz), (0, 45 if sx * sz > 0 else -45, 0))
        m.box(name, (cx - 1.5, 36, cz), (cx + 1.5, 39, cz + 11 * sz), IRON_DARK, "metal", {"front": [("rect", 1, 1, 1, 1, ETHER)]})
        m.pop()
    m.box("root", (-half, 0, -half), (half, 0.5, half), "#3a3d44", "stone", faces=("top",), res=0.25)
    rise = [("root", "translation", [(0, [0, -40, 0]), (0.8, [0, 2, 0]), (1.0, [0, 0, 0])])]
    m.clip("rise", 1.0, rise)
    m.clip("fire", 0.4, [(n, "rotation", [(0, [0, 0, 0]), (0.05, [-10, 0, 0]), (0.4, [0, 0, 0])]) for n in ("cannonNE", "cannonNW", "cannonSE", "cannonSW")])
    return m


def stone_wall():
    """Mur de pierre du Sorcier (Rempart Tellurique, Mur de Magma) : 5 blocs de long, 3 de haut,
    1 d'épaisseur, en blocs de roche mal équarris. `rise` le fait surgir."""
    m = Model("stoneWall", seed=11)
    m.bone("root", None, (0, 0, 0))
    rng = m.rng
    y = 0
    for row, h in enumerate((10, 9, 10, 9, 10)):
        x = -40 + (row % 2) * 6
        while x < 40:
            w = rng.choice((10, 12, 14, 16))
            x1 = min(40, x + w)
            depth = rng.uniform(-1.2, 1.2)
            c = rng.choice(("#7a746a", "#6e685e", "#837d72", "#69645b"))
            top = y + h + (rng.uniform(-1.5, 2.5) if row == 4 else 0)
            m.box("root", (x, y, -8 + depth), (x1, top, 8 + depth), c, "stone", {"sides": [("frame", "#4e4a43")]}, res=0.75)
            x = x1
        y += h
    # Mousse et terre arrachée à la base
    m.box("root", (-42, 0, -10), (42, 3, 10), "#5a4632", "moss", res=0.5)
    m.clip("rise", 0.6, [("root", "translation", [(0, [0, -48, 0]), (0.45, [0, 3, 0]), (0.6, [0, 0, 0])])])
    return m


ALL = [ballistic_turret, flame_turret, jaw_trap, magnetic_trap, shrapnel_mine, portable_barricade, etherium_bastion, stone_wall]
