#!/usr/bin/env python3
"""Planche de contrôle des gestes du héros : chaque clé de chaque geste, en bâtonnets, de profil
(l'avant à droite) et de face. Ce n'est pas le rendu du jeu, seulement de quoi relire les poses
en attendant le lecteur de gestes.

    python3 tools/planche_gestes.py [dossier des gestes] [image de sortie]
"""
import json, math, glob, sys
import numpy as np
from PIL import Image, ImageDraw, ImageFont
def Rx(a): a=math.radians(a); c,s=math.cos(a),math.sin(a); return np.array([[1,0,0],[0,c,-s],[0,s,c]])
def Ry(a): a=math.radians(a); c,s=math.cos(a),math.sin(a); return np.array([[c,0,s],[0,1,0],[-s,0,c]])
def Rz(a): a=math.radians(a); c,s=math.cos(a),math.sin(a); return np.array([[c,-s,0],[s,c,0],[0,0,1]])
# sens « naturel » de chaque os : signe appliqué à x et z
SIGN = {"body":(1,1),"torso":(1,-1),"head":(1,-1),
        "rightArm":(-1,-1),"leftArm":(-1,1),"rightForearm":(-1,-1),"leftForearm":(-1,1),
        "rightHand":(-1,-1),"leftHand":(-1,1),"rightLeg":(-1,-1),"leftLeg":(-1,1),"rightShin":(1,-1),"leftShin":(1,1)}
def rot(bone, r):
    sx, sz = SIGN[bone]
    return Ry(r[1]) @ Rx(sx*r[0]) @ Rz(sz*r[2])
def pose_at(keys, t):
    for a, b in zip(keys, keys[1:]):
        if a["t"] <= t <= b["t"]:
            u = (t-a["t"])/(b["t"]-a["t"]) if b["t"]>a["t"] else 1
            e = b.get("ease","inOut")
            u = {"linear":u,"in":u*u,"out":1-(1-u)**2,"inOut":u*u*(3-2*u)}[e]
            out = {}
            for bone in set(a.get("bones",{}))|set(b.get("bones",{})):
                pa = a.get("bones",{}).get(bone,{}); pb = b.get("bones",{}).get(bone,{})
                out[bone] = {k: [x+(y-x)*u for x,y in zip(pa.get(k,[0,0,0]), pb.get(k,[0,0,0]))] for k in ("rotation","translation")}
            return out
    return {}
def skeleton(p):
    g = lambda b: p.get(b, {"rotation":[0,0,0],"translation":[0,0,0]})
    R0 = rot("body", g("body")["rotation"]); T0 = np.array(g("body")["translation"])
    W = lambda v: R0 @ np.array(v) + T0
    segs = []
    hips = np.array([0,0.9,0])
    Rt = R0 @ rot("torso", g("torso")["rotation"])
    T = lambda v: R0 @ hips + T0 + Rt @ (np.array(v) - hips)
    neck = T([0,1.42,0]); segs.append(("c", T([0,0.9,0]), neck))
    Rh = Rt @ rot("head", g("head")["rotation"])
    segs.append(("h", neck, neck + Rh @ np.array([0,0.32,0])))
    segs.append(("c", T([0.25,1.4,0]), T([-0.25,1.4,0])))
    for side, sx, col in (("right",-0.25,"r"),("left",0.25,"l")):
        sh = T([sx,1.4,0]); Ra = Rt @ rot(side+"Arm", g(side+"Arm")["rotation"])
        el = sh + Ra @ np.array([0,-0.3,0]); Rf = Ra @ rot(side+"Forearm", g(side+"Forearm")["rotation"])
        wr = el + Rf @ np.array([0,-0.3,0]); Rw = Rf @ rot(side+"Hand", g(side+"Hand")["rotation"])
        segs += [(col, sh, el), (col, el, wr)]
        if side == "right": segs.append(("w", wr, wr + Rw @ np.array([0,0.15,0.7])))
        hp = W([sx*0.4,0.9,0]); Rl = R0 @ rot(side+"Leg", g(side+"Leg")["rotation"])
        kn = hp + Rl @ np.array([0,-0.45,0]); Rs = Rl @ rot(side+"Shin", g(side+"Shin")["rotation"])
        segs += [(col, W([0,0.9,0]), hp), (col, hp, kn), (col, kn, kn + Rs @ np.array([0,-0.45,0]))]
    return segs
COL = {"c":(60,60,70),"h":(60,60,70),"r":(200,60,50),"l":(50,100,200),"w":(170,130,40)}
def draw(d, segs, ox, oy, view, sc=62):
    d.line([(ox-65, oy), (ox+65, oy)], fill=(200,200,200))
    for c, a, b in segs:
        P = lambda v: (ox + (v[2] if view=="side" else -v[0])*sc, oy - v[1]*sc)
        d.line([P(a), P(b)], fill=COL[c], width=4 if c=="h" else 3)
SRC = sys.argv[1] if len(sys.argv) > 1 else "assets/hero/animations"
OUT = sys.argv[2] if len(sys.argv) > 2 else "planche-gestes.png"
files = sorted(glob.glob(SRC + "/*.json"))
cols = 6; cw = 150; lw = 170; rh = 165
img = Image.new("RGB", (lw + cols*2*cw, len(files)*rh + 30), "white"); d = ImageDraw.Draw(img)
try: font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 13)
except Exception: font = None
d.text((10, 8), "Rouge : côté droit · bleu : gauche · or : arme · chaque instant vu de profil (avant à droite) puis de face", fill=(80,80,80), font=font)
for i, f in enumerate(files):
    g = json.load(open(f)); y0 = 30 + i*rh
    times = [k["t"] for k in g["keys"]]
    if "impact" in g and g["impact"] not in times: times = sorted(times + [g["impact"]])
    times = times[:cols]
    d.text((10, y0+40), f.split("/")[-1][:-5], fill=(0,0,0), font=font)
    d.text((10, y0+58), f"{g['duration']} s · {g['legs']}", fill=(110,110,110), font=font)
    for j, t in enumerate(times):
        segs = skeleton(pose_at(g["keys"], t)); x = lw + j*2*cw
        draw(d, segs, x + cw//2, y0 + 140, "side"); draw(d, segs, x + cw + cw//2, y0 + 140, "front")
        mark = " impact" if abs(t - g.get("impact", -1)) < 1e-6 else ""
        d.text((x + 4, y0 + 2), f"t = {t:g}{mark}", fill=(200,40,40) if mark else (90,90,90), font=font)
img.save(OUT); print(img.size)
