#!/usr/bin/env python3
# Copyright 2026 The Terasology Foundation
# SPDX-License-Identifier: Apache-2.0
"""Ajoure les tuiles de feuillage : des trous repartis, pour voir a travers.

    python tools/feuillage/ajourer.py

Part des tuiles d'origine de CoreAssets, gardees dans origine/, et ecrit les
tuiles ajourees dans modules/CoreAssets/assets/blockTiles/plant/foliage/.
Les trous sont semes uniformement sur le tore 16x16 — la tuile se repete
sans bande vide — et le texel sous un trou s'eclaire, celui au-dessus
s'assombrit : la lumiere vient d'en haut. Le tirage est fixe, donc relancer
le script redonne les memes images au bit pres.
"""
from __future__ import annotations

import pathlib
import random

from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parents[2]
ORIGINE = pathlib.Path(__file__).resolve().parent / "origine"
SORTIE = ROOT / "modules/CoreAssets/assets/blockTiles/plant/foliage"

N = 16
TUILES = ["GreenLeaf", "DarkGreenLeaf", "LightGreenLeaf", "RedLeaf"]
# Part de la tuile qui reste pleine : deux cinquiemes de jour.
COUVERTURE = 0.64
# Ecart minimal entre deux trous, en texels, sur le tore.
ECART = 2.8


def masque(graine: int) -> list[list[bool]]:
    """Vrai la ou la feuille est pleine."""
    rng = random.Random(graine)
    plein = [[True] * N for _ in range(N)]
    centres: list[tuple[int, int]] = []

    def isole(x: int, y: int) -> bool:
        for cx, cy in centres:
            dx = min(abs(x - cx), N - abs(x - cx))
            dy = min(abs(y - cy), N - abs(y - cy))
            if dx * dx + dy * dy < ECART * ECART:
                return False
        return True

    essais = 0
    while sum(map(sum, plein)) > COUVERTURE * N * N and essais < 20000:
        essais += 1
        x, y = rng.randrange(N), rng.randrange(N)
        if not isole(x, y):
            continue
        centres.append((x, y))
        trou = [(x, y)]
        for _ in range(rng.randrange(4, 8)):
            bx, by = rng.choice(trou)
            dx, dy = rng.choice([(1, 0), (-1, 0), (0, 1), (0, -1)])
            trou.append(((bx + dx) % N, (by + dy) % N))
        for tx, ty in trou:
            plein[ty][tx] = False
    # Un texel plein qui ne tient a rien n'est pas une feuille, c'est du bruit : il tombe.
    for y in range(N):
        for x in range(N):
            voisins = sum(plein[(y + dy) % N][(x + dx) % N] for dx, dy in [(1, 0), (-1, 0), (0, 1), (0, -1)])
            if plein[y][x] and voisins < 2:
                plein[y][x] = False
    return plein


def ajourer(nom: str, graine: int) -> Image.Image:
    source = Image.open(ORIGINE / f"{nom}.png").convert("RGBA")
    teintes = sorted({p[:3] for p in source.getdata() if p[3] > 127}, key=sum)
    plein = masque(graine)
    rng = random.Random(graine + 1)
    tuile = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    for y in range(N):
        for x in range(N):
            if not plein[y][x]:
                continue
            r, g, b, a = source.getpixel((x, y))
            couleur = (r, g, b) if a > 127 else teintes[len(teintes) // 2]
            if not plein[(y - 1) % N][x]:
                couleur = teintes[-1 - rng.randrange(2)]
            elif not plein[(y + 1) % N][x]:
                couleur = teintes[rng.randrange(2)]
            tuile.putpixel((x, y), couleur + (255,))
    return tuile


def main() -> None:
    for i, nom in enumerate(TUILES):
        tuile = ajourer(nom, 1000 + i)
        tuile.save(SORTIE / f"{nom}.png")
        jour = sum(1 for p in tuile.getdata() if p[3] < 128) / (N * N)
        print(f"{nom}: {jour:.0%} de jour")


if __name__ == "__main__":
    main()
