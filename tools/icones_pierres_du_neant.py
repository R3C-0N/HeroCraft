#!/usr/bin/env python3
"""Dessine au pixel près les icônes 16 × 16 des Pierres du Néant.

    python3 tools/icones_pierres_du_neant.py

Écrit `assets/items/icones/voidStone.png` (la Pierre entière : un cristal à facettes, noir
violacé, au cœur lumineux) et `assets/items/icones/voidStoneFragment.png` (un petit éclat brisé).
"""
from pathlib import Path

from PIL import Image

ICONES = Path(__file__).resolve().parent.parent / "assets/items/icones"

# Palette : contour presque noir, facettes sombres et claires, veines et cœur lumineux
PALETTE = {
    "#": (18, 8, 30, 255),     # contour
    "d": (44, 20, 70, 255),    # facette dans l'ombre
    "m": (78, 36, 120, 255),   # facette médiane
    "l": (128, 70, 190, 255),  # facette éclairée
    "h": (196, 150, 245, 255), # reflet
    "v": (150, 90, 255, 255),  # veine
    "c": (230, 210, 255, 255), # cœur
    "w": (255, 255, 255, 255), # éclat du cœur
}

PIERRE = [
    "................",
    ".......##.......",
    "......#hl#......",
    ".....#hlmd#.....",
    "....#hlmmdd#....",
    "...#hlvmmddd#...",
    "...#lmvcvmdd#...",
    "..#llvcwcvmdd#..",
    "..#lmmvcvmmdd#..",
    "...#mmmvmmdd#...",
    "...#dmmmmddd#...",
    "....#dmmddd#....",
    ".....#dddd#.....",
    "......#dd#......",
    ".......##.......",
    "................",
]

FRAGMENT = [
    "................",
    "................",
    "................",
    "................",
    "................",
    "........#.......",
    ".......#h#......",
    "......#hld#.....",
    "......#lcd#.....",
    "......#vwd#.....",
    ".......#md#.....",
    ".......###......",
    "................",
    "................",
    "................",
    "................",
]


def dessine(motif):
    image = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
    for y, ligne in enumerate(motif):
        assert len(ligne) == 16, ligne
        for x, c in enumerate(ligne):
            if c != ".":
                image.putpixel((x, y), PALETTE[c])
    return image


if __name__ == "__main__":
    ICONES.mkdir(parents=True, exist_ok=True)
    dessine(PIERRE).save(ICONES / "voidStone.png", optimize=True)
    dessine(FRAGMENT).save(ICONES / "voidStoneFragment.png", optimize=True)
    print("Icônes écrites dans", ICONES)
