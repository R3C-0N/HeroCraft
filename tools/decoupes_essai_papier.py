"""Découpes de la scène d'essai des cartes de papier (assets/cinematics/cards/essai-*.png).

Des formes simples à la façon des maquettes HTML (papier découpé, aplats et quelques traits),
pour essayer le rendu des cartes en attendant les découpes tirées des maquettes.
Usage : python3 tools/decoupes_essai_papier.py
"""
import math
import random
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

OUT = Path(__file__).resolve().parent.parent / "assets" / "cinematics" / "cards"
S = 4  # suréchantillonnage, pour des bords nets une fois réduits


def save(img, name, size):
    img = img.resize(size, Image.LANCZOS)
    img.save(OUT / name)


def speckle(img, color, count, rng, radius=(1, 3)):
    d = ImageDraw.Draw(img)
    mask = img.getchannel("A")
    w, h = img.size
    for _ in range(count):
        x, y = rng.randrange(w), rng.randrange(h)
        if mask.getpixel((x, y)) > 200:
            r = rng.randint(*radius) * S
            d.ellipse([x - r, y - r, x + r, y + r], fill=color)


def planet(rng):
    w = 512 * S
    img = Image.new("RGBA", (w, w), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    c, r = w / 2, w * 0.46
    d.ellipse([c - r, c - r, c + r, c + r], fill=(178, 70, 32, 255))
    # Croûte sombre, plaques irrégulières
    for _ in range(14):
        a = rng.uniform(0, math.tau)
        dist = rng.uniform(0, r * 0.75)
        pr = rng.uniform(r * 0.12, r * 0.3)
        x, y = c + math.cos(a) * dist, c + math.sin(a) * dist
        pts = [(x + math.cos(t) * pr * rng.uniform(0.7, 1.2), y + math.sin(t) * pr * rng.uniform(0.7, 1.2)) for t in [i * math.tau / 9 for i in range(9)]]
        d.polygon(pts, fill=(96, 34, 20, 255))
    # Failles de lave
    for _ in range(9):
        x, y = c + rng.uniform(-r, r) * 0.6, c + rng.uniform(-r, r) * 0.6
        pts = [(x, y)]
        for _ in range(6):
            x += rng.uniform(-60, 60) * S
            y += rng.uniform(-60, 60) * S
            pts.append((x, y))
        d.line(pts, fill=(255, 168, 60, 255), width=5 * S, joint="curve")
    # Ombre du terminateur
    shade = Image.new("L", (w, w), 0)
    ImageDraw.Draw(shade).ellipse([c - r * 0.55, c - r * 1.05, c + r * 1.45, c + r * 0.95], fill=150)
    shade = shade.filter(ImageFilter.GaussianBlur(40 * S))
    dark = Image.new("RGBA", (w, w), (40, 12, 8, 255))
    disc = Image.new("L", (w, w), 0)
    ImageDraw.Draw(disc).ellipse([c - r, c - r, c + r, c + r], fill=255)
    from PIL import ImageChops
    img.paste(dark, (0, 0), ImageChops.multiply(shade, disc))
    # Rien ne déborde du disque
    img.putalpha(ImageChops.multiply(img.getchannel("A"), disc))
    save(img, "essai-planete.png", (512, 512))


def ridge(rng, name, color, height, peaks, size, detail=None):
    w, h = size[0] * S, size[1] * S
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    pts = [(0, h)]
    n = peaks * 2 + 1
    for i in range(n + 1):
        x = w * i / n
        top = h * (1 - height) if i % 2 == 1 else h * (1 - height * rng.uniform(0.35, 0.6))
        pts.append((x, top + rng.uniform(-0.04, 0.04) * h))
    pts.append((w, h))
    d.polygon(pts, fill=color)
    if detail:
        speckle(img, detail, 120, rng)
    save(img, name, size)


def sun(rng):
    w = 256 * S
    img = Image.new("RGBA", (w, w), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    c = w / 2
    for i in range(16):
        a = i * math.tau / 16
        r0, r1 = w * 0.30, w * (0.47 if i % 2 == 0 else 0.40)
        da = math.tau / 48
        d.polygon([(c + math.cos(a - da) * r0, c + math.sin(a - da) * r0), (c + math.cos(a) * r1, c + math.sin(a) * r1), (c + math.cos(a + da) * r0, c + math.sin(a + da) * r0)], fill=(255, 196, 92, 255))
    r = w * 0.31
    d.ellipse([c - r, c - r, c + r, c + r], fill=(255, 214, 122, 255))
    save(img, "essai-soleil.png", (256, 256))


def cloud(rng):
    w, h = 512 * S, 220 * S
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    base = h * 0.78
    d.rounded_rectangle([w * 0.06, base - h * 0.2, w * 0.94, base], radius=h * 0.1, fill=(246, 238, 220, 255))
    for x, r in [(0.22, 0.26), (0.40, 0.38), (0.60, 0.32), (0.78, 0.22)]:
        d.ellipse([w * x - h * r, base - h * r * 2, w * x + h * r, base], fill=(246, 238, 220, 255))
    save(img, "essai-nuage.png", (512, 220))


def tree(rng):
    w, h = 220 * S, 400 * S
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rectangle([w * 0.44, h * 0.55, w * 0.56, h], fill=(74, 48, 30, 255))
    for i, (y, s) in enumerate([(0.55, 0.48), (0.38, 0.40), (0.22, 0.30)]):
        d.polygon([(w * 0.5 - w * s, h * y + h * 0.12), (w * 0.5, h * (y - 0.2)), (w * 0.5 + w * s, h * y + h * 0.12)], fill=(58 + i * 10, 92 + i * 8, 52, 255))
    save(img, "essai-sapin.png", (220, 400))


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    rng = random.Random(7)
    planet(rng)
    ridge(rng, "essai-montagnes.png", (88, 62, 48, 255), 0.9, 3, (1024, 340), (70, 48, 38, 255))
    ridge(rng, "essai-collines.png", (170, 128, 74, 255), 0.6, 4, (1024, 260), (150, 110, 62, 255))
    sun(rng)
    cloud(rng)
    tree(rng)
