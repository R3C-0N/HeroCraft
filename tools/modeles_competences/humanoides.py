"""Les invocations humanoïdes du Nécromancien, sur le corps du héros (13 os, `hero_model.rs`) :
une planche 64 × 64 au patron du héros (`assets/heros/atlas.json`), peinte ici, et l'espèce dont
elles prennent les proportions. Les armes qu'elles tiennent sont des pièces greffées sur la main
droite (`rightHand`), comme les greffes du Druide.
"""
import json
import pathlib

import numpy as np
from PIL import Image

from boites import apply_decal, paint_pattern, rgb
from greffes import graft

ATLAS = json.loads((pathlib.Path(__file__).resolve().parents[2] / "assets/heros/atlas.json").read_text())
PARTS = ATLAS["PATRON"]["parties"]
FACE_NAMES = {"front": "devant", "back": "derriere", "left": "gauche", "right": "droite", "top": "dessus", "bottom": "dessous"}


class Sheet:
    def __init__(self, ident, seed=1):
        import random
        self.ident = ident
        self.img = np.zeros((64, 64, 4), np.uint8)
        self.rng = random.Random(seed)

    def paint(self, part, color, pattern="plain", decals=None, faces=None):
        """Peint les faces d'une partie du patron (`tete`, `torse2`, `brasD`…). `decals` par face
        (`front`, `back`, `left`, `right`, `top`, `bottom`, `sides`, `all`)."""
        decals = decals or {}
        for face in faces or FACE_NAMES:
            r = PARTS[part]["faces"][FACE_NAMES[face]]
            x, y, w, h = r["x"], r["y"], r["largeur"], r["hauteur"]
            img = np.zeros((h, w, 4), np.uint8)
            paint_pattern(img, pattern, rgb(color) if isinstance(color, str) else color, self.rng, face)
            for d in decals.get(face, []) + decals.get("all", []) + (decals.get("sides", []) if face in ("front", "back", "left", "right") else []):
                apply_decal(img, d)
            self.img[y:y + h, x:x + w] = img

    def region(self, x, y, w, h, fn):
        """Peint une case libre (barbe, appendice, queue…) : `fn(img)` sur un tableau h × w × 4."""
        img = np.zeros((h, w, 4), np.uint8)
        fn(img)
        self.img[y:y + h, x:x + w] = img

    def save(self, path):
        Image.fromarray(self.img, "RGBA").save(path, optimize=True)


def pixels(rows, palette):
    """Une case en pixel-art : `rows` (chaînes) et `palette` {caractère: couleur}, `.` transparent."""
    def fn(img):
        for j, row in enumerate(rows):
            for i, ch in enumerate(row):
                if ch != ".":
                    img[j, i] = rgb(palette[ch])
    return fn


BONE = "#ddd3bd"
GAP = "#2b2622"


def skeleton_servant():
    s = Sheet("skeletonServant")
    eye = "#7dff9a"
    s.paint("tete", BONE, "bone", {
        "front": [("rect", 1, 2, 2, 2, "#141110"), ("rect", 5, 2, 2, 2, "#141110"), ("px", 2, 3, eye), ("px", 5, 3, eye),
                  ("px", 3, 5, "#141110"), ("px", 4, 5, "#141110"), ("row", 6, "#3a332c"), ("pixels", [(1, 6), (3, 6), (5, 6), (7, 6)], BONE),
                  ("row", 7, "#c9bea4")],
        "top": [("px", 2, 3, "#bfb294"), ("px", 3, 3, "#bfb294")]})
    # Cage thoracique : côtes claires sur fond sombre, colonne au milieu
    ribs = [("rect", 0, 0, 99, 99, GAP)] + [("row", y, BONE) for y in (1, 3, 5, 7)] + [("col", 3, BONE), ("col", 4, BONE), ("rect", 1, 9, 6, 2, "#c9bea4")]
    s.paint("torse", BONE, "bone", {"front": ribs, "back": [("rect", 0, 0, 99, 99, GAP), ("col", 3, BONE), ("col", 4, BONE)] + [("row", y, "#c9bea4") for y in (1, 4, 7)],
                                    "left": [("rect", 0, 0, 99, 99, GAP)] + [("row", y, BONE) for y in (1, 3, 5, 7)],
                                    "right": [("rect", 0, 0, 99, 99, GAP)] + [("row", y, BONE) for y in (1, 3, 5, 7)]})
    limb = {"sides": [("col", 0, GAP), ("col", 3, GAP), ("rect", 0, 4, 4, 1, "#c9bea4")]}
    for part in ("brasD", "brasG", "jambeD", "jambeG"):
        s.paint(part, BONE, "bone", limb)
    # Couche 2 : pagne en lambeaux, ceinture, épaulière rouillée
    s.paint("torse2", (0, 0, 0, 0), "flat", {
        "front": [("rect", 0, 9, 8, 1, "#5a3e24"), ("pixels", [(0, 10), (1, 10), (2, 11), (5, 10), (6, 10), (6, 11)], "#6e4a2a")],
        "back": [("rect", 0, 9, 8, 1, "#5a3e24"), ("pixels", [(1, 10), (3, 10), (3, 11), (6, 10)], "#6e4a2a")],
        "left": [("rect", 0, 9, 4, 1, "#5a3e24")], "right": [("rect", 0, 9, 4, 1, "#5a3e24")]})
    s.paint("brasD2", (0, 0, 0, 0), "flat", {"sides": [("rect", 0, 0, 4, 3, "#7a5a3a"), ("pixels", [(1, 1), (2, 2)], "#a0703a")],
                                            "top": [("rect", 0, 0, 4, 4, "#7a5a3a")]})
    return s


def death_knight():
    s = Sheet("deathKnight")
    plate, dark, trim, glow = "#3a3d46", "#22242a", "#6a7080", "#64e8ff"
    s.paint("tete", plate, "metal", {"front": [("rect", 0, 3, 8, 1, "#0c0d10"), ("rect", 1, 3, 2, 1, glow), ("rect", 5, 3, 2, 1, glow),
                                               ("col", 3, dark), ("col", 4, dark), ("rect", 1, 5, 6, 2, dark), ("pixels", [(2, 5), (4, 5), (5, 6), (3, 6)], "#0c0d10")]})
    s.paint("torse", plate, "metal", {"front": [("col", 3, trim), ("col", 4, trim), ("row", 5, dark), ("rect", 2, 1, 4, 3, dark), ("rect", 3, 2, 2, 1, glow)],
                                      "back": [("col", 3, dark), ("col", 4, dark)]})
    for part in ("brasD", "brasG"):
        s.paint(part, plate, "metal", {"sides": [("row", 5, trim), ("rect", 0, 9, 4, 3, dark)]})
    for part in ("jambeD", "jambeG"):
        s.paint(part, plate, "metal", {"sides": [("row", 6, trim), ("rect", 0, 10, 4, 2, dark)]})
    # Couche 2 : heaume à cimier, épaulières à pointes, tassettes, cape
    s.paint("tete2", (0, 0, 0, 0), "flat", {"front": [("rect", 0, 0, 8, 2, trim), ("px", 0, 2, trim), ("px", 7, 2, trim)],
                                            "left": [("rect", 0, 0, 8, 2, trim)], "right": [("rect", 0, 0, 8, 2, trim)],
                                            "back": [("rect", 0, 0, 8, 3, trim)], "top": [("rect", 3, 0, 2, 8, "#8b1c1c")]})
    for part in ("brasD2", "brasG2"):
        s.paint(part, (0, 0, 0, 0), "flat", {"sides": [("rect", 0, 0, 4, 4, trim), ("px", 1, 0, "#a0a8b8"), ("px", 2, 0, "#a0a8b8")],
                                            "top": [("rect", 0, 0, 4, 4, trim), ("px", 1, 1, "#c0c8d8"), ("px", 2, 2, "#c0c8d8")]})
    s.paint("torse2", (0, 0, 0, 0), "flat", {"front": [("rect", 0, 8, 8, 4, dark), ("col", 3, (0, 0, 0, 0)), ("col", 4, (0, 0, 0, 0))],
                                             "back": [("rect", 0, 0, 8, 12, "#5a1414"), ("pixels", [(1, 11), (4, 11), (6, 11)], (0, 0, 0, 0))],
                                             "left": [("rect", 0, 8, 4, 4, dark)], "right": [("rect", 0, 8, 4, 4, dark)]})
    # Cornes du heaume (appendice crânien)
    s.region(32, 0, 8, 8, pixels([
        "........",
        "#......#",
        "#......#",
        "##....##",
        ".##..##.",
        ".##..##.",
        "..#..#..",
        "........",
    ], {"#": "#c9c2b0"}))
    return s


def lich_lord():
    s = Sheet("lichLord")
    robe, deep, gold, skull, soul = "#3b2550", "#24163a", "#d4a63a", "#d8d0bc", "#7dff9a"
    s.paint("tete", skull, "bone", {"front": [("rect", 1, 3, 2, 2, "#0e0b10"), ("rect", 5, 3, 2, 2, "#0e0b10"), ("px", 2, 4, soul), ("px", 5, 4, soul),
                                              ("px", 3, 5, "#0e0b10"), ("px", 4, 5, "#0e0b10"), ("row", 7, "#3a332c"), ("pixels", [(1, 7), (3, 7), (5, 7)], skull)]})
    s.paint("torse", robe, "plain", {"front": [("col", 3, gold), ("col", 4, deep), ("rect", 2, 2, 4, 2, deep), ("px", 3, 3, soul), ("px", 4, 3, soul)]})
    for part in ("brasD", "brasG"):
        s.paint(part, robe, "plain", {"sides": [("rect", 0, 9, 4, 3, skull), ("row", 8, gold)]})
    for part in ("jambeD", "jambeG"):
        s.paint(part, robe, "plain", {"sides": [("row", -1, gold)]})
    # Couche 2 : capuche ouverte sur le crâne, robe longue, manches évasées, étole
    s.paint("tete2", deep, "plain", {"front": [("rect", 1, 1, 6, 7, (0, 0, 0, 0)), ("rect", 0, 0, 8, 1, gold)], "bottom": [("rect", 0, 0, 8, 8, (0, 0, 0, 0))]})
    s.paint("torse2", robe, "plain", {"front": [("col", 3, gold), ("col", 4, gold), ("row", -1, gold)], "back": [("row", -1, gold)]})
    for part in ("jambeD2", "jambeG2"):
        s.paint(part, robe, "plain", {"sides": [("row", -1, gold), ("row", -2, deep)], "bottom": [("rect", 0, 0, 4, 4, (0, 0, 0, 0))]})
    for part in ("brasD2", "brasG2"):
        s.paint(part, robe, "plain", {"sides": [("rect", 0, 9, 4, 3, (0, 0, 0, 0)), ("row", 8, gold)], "bottom": [("rect", 0, 0, 4, 4, (0, 0, 0, 0))]})
    # Couronne (appendice crânien)
    s.region(32, 0, 8, 8, pixels([
        "........",
        "........",
        "........",
        "#..#...#",
        "#.###.##",
        "##g#####",
        "########",
        "........",
    ], {"#": gold, "g": soul}))
    return s


def bone_sword():
    """Épée courte du squelette, tenue dans la main droite."""
    m = graft("boneSword", ["rightHand"])
    m.box("rightHand", (-7, 12, -1), (-5, 13, 3), "#5a3e24", "wood")
    m.box("rightHand", (-7.5, 12.5, 3), (-4.5, 13.5, 4), "#8a7a5a", "metal")
    m.box("rightHand", (-6.5, 12.6, 4), (-5.5, 13.4, 13), "#b8bcc2", "metal", {"top": [("pixels", [(0, 2), (0, 5)], "#7a6a55")]})
    return m


def runeblade():
    """Grande épée runique du chevalier de la mort."""
    m = graft("runeblade", ["rightHand"])
    m.box("rightHand", (-7, 12, -3), (-5, 13, 3), "#2a2224", "wood")
    m.box("rightHand", (-8.5, 12.3, 3), (-3.5, 13.7, 4.2), "#6a7080", "metal")
    m.box("rightHand", (-7.2, 12.4, 4.2), (-4.8, 13.6, 21), "#4a5060", "metal", {"top": [("pixels", [(1, k) for k in range(2, 16, 3)], "#64e8ff")]})
    return m


def lich_staff():
    """Bâton du seigneur liche, coiffé d'un orbe d'âme."""
    m = graft("lichStaff", ["rightHand"])
    m.beam("rightHand", (-6, 2, 1), (-6, 26, 1), 1.2, color="#3a2a20", pattern="wood")
    for a in (0, 45):
        m.box("rightHand", (-7.5, 25, -0.5), (-4.5, 28, 2.5), "#7dff9a", "glow", {"all": [("px", 0, 0, "#e0ffe8")]}, euler=(0, a, 0))
    m.box("rightHand", (-7, 24, 0), (-5, 25, 2), "#d4a63a", "metal")
    return m


SHEETS = [skeleton_servant, death_knight, lich_lord]
WEAPONS = [bone_sword, runeblade, lich_staff]
