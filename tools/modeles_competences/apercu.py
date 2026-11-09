"""Aperçu des modèles glTF générés, sans lancer le jeu : vue de trois quarts, z-buffer, texels au
plus proche, ombrage simple. Sert aux planches de relecture.

    python3 tools/modeles_competences/apercu.py sortie.png modele1.gltf modele2.gltf …
"""
import base64
import json
import pathlib
import struct
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFont

from boites import quat_from_matrix, rot_matrix  # noqa: F401

BG = (205, 225, 238)


def load(path):
    g = json.loads(pathlib.Path(path).read_text())
    blob = base64.b64decode(g["buffers"][0]["uri"].split(",", 1)[1])

    def acc(i):
        a = g["accessors"][i]
        v = g["bufferViews"][a["bufferView"]]
        n = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}[a["type"]]
        fmt = {5126: "f", 5123: "H", 5125: "I", 5121: "B"}[a["componentType"]]
        data = struct.unpack_from("<%d%s" % (a["count"] * n, fmt), blob, v["byteOffset"] + a.get("byteOffset", 0))
        return np.array(data).reshape(a["count"], n)

    prim = g["meshes"][0]["primitives"][0]
    at = prim["attributes"]
    tex = np.array(Image.open(pathlib.Path(path).with_suffix(".png")).convert("RGBA"))
    joints = g["skins"][0]["joints"]
    return {
        "pos": acc(at["POSITION"]),
        "nor": acc(at["NORMAL"]),
        "uv": acc(at["TEXCOORD_0"]),
        "joint": acc(at["JOINTS_0"])[:, 0].astype(int),
        "idx": acc(prim["indices"])[:, 0].astype(int),
        "tex": tex,
        "nodes": g["nodes"],
        "joints": joints,
        "gltf": g,
        "acc": acc,
    }


def quat_to_matrix(q):
    x, y, z, w = q
    return np.array([
        [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
        [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
        [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
    ])


def clip_pose(m, clip, t):
    """Rotation et translation de chaque nœud à l'instant `t` d'une animation glTF."""
    pose = {}
    for anim in m["gltf"].get("animations", []):
        if anim["name"] != clip:
            continue
        for ch in anim["channels"]:
            s = anim["samplers"][ch["sampler"]]
            times = m["acc"](s["input"])[:, 0]
            vals = m["acc"](s["output"])
            k = np.searchsorted(times, t)
            if k <= 0:
                v = vals[0]
            elif k >= len(times):
                v = vals[-1]
            else:
                f = (t - times[k - 1]) / (times[k] - times[k - 1])
                v = vals[k - 1] * (1 - f) + vals[k] * f
                if ch["target"]["path"] == "rotation":
                    v = v / np.linalg.norm(v)
            pose.setdefault(ch["target"]["node"], {})[ch["target"]["path"]] = v
    return pose


def skin(m, pose=None, euler=None):
    """Positions et normales dans la pose : `pose` (d'une animation glTF) ou `euler` {nom d'os: [x, y, z]}."""
    nodes = m["nodes"]
    pose = pose or {}
    euler = euler or {}
    parent = {}
    for i, n in enumerate(nodes):
        for c in n.get("children", []):
            parent[c] = i
    world = {}
    rest_world = {}

    def compute(i, rest):
        cache = rest_world if rest else world
        if i in cache:
            return cache[i]
        n = nodes[i]
        t = np.array(n.get("translation", [0, 0, 0]), float)
        r = quat_to_matrix(n.get("rotation", [0, 0, 0, 1]))
        if not rest:
            p = pose.get(i, {})
            if "translation" in p:
                t = p["translation"]
            if "rotation" in p:
                r = quat_to_matrix(p["rotation"])
            if n.get("name") in euler:
                r = r @ rot_matrix(euler[n["name"]])
        mat = np.eye(4)
        mat[:3, :3] = r
        mat[:3, 3] = t
        if i in parent:
            mat = compute(parent[i], rest) @ mat
        cache[i] = mat
        return mat

    skins = []
    for j in m["joints"]:
        skins.append(compute(j, False) @ np.linalg.inv(compute(j, True)))
    skins = np.array(skins)
    mats = skins[m["joint"]]
    pos = np.einsum("nij,nj->ni", mats[:, :3, :3], m["pos"]) + mats[:, :3, 3]
    nor = np.einsum("nij,nj->ni", mats[:, :3, :3], m["nor"])
    return pos, nor


def render(m, size, yaw=0.6, pitch=0.5, scale=None, pose=None, euler=None, tint=None, center=None):
    pos, nor = skin(m, pose, euler)
    cy, sy = np.cos(yaw), np.sin(yaw)
    cp, sp = np.cos(pitch), np.sin(pitch)
    # Rotation autour de y (cap), puis inclinaison vers le bas
    x = pos[:, 0] * cy + pos[:, 2] * sy
    z = -pos[:, 0] * sy + pos[:, 2] * cy
    y = pos[:, 1]
    sy_ = y * cp - z * sp
    d = y * sp + z * cp  # vers la caméra : grand d = proche
    nx = nor[:, 0] * cy + nor[:, 2] * sy
    nz = -nor[:, 0] * sy + nor[:, 2] * cy
    ny = nor[:, 1]
    light = np.array([0.45, 0.75, 0.5])
    light /= np.linalg.norm(light)
    W = H = size
    if scale is None:
        ext = max(np.ptp(x), np.ptp(sy_), 1e-3)
        scale = size * 0.78 / ext
    cx = (x.max() + x.min()) / 2 if center is None else center[0]
    cyy = (sy_.max() + sy_.min()) / 2 if center is None else center[1]
    px = W / 2 + (x - cx) * scale
    py = H / 2 - (sy_ - cyy) * scale
    img = np.zeros((H, W, 4), np.uint8)
    depth = np.full((H, W), -1e9)
    tex = m["tex"]
    th, tw = tex.shape[:2]
    for t in range(0, len(m["idx"]), 3):
        ids = m["idx"][t:t + 3]
        n = np.array([nx[ids[0]], ny[ids[0]], nz[ids[0]]])
        shade = 0.6 + 0.4 * max(0.0, float(n @ light)) + 0.1 * max(0.0, n[1])
        xs, ys = px[ids], py[ids]
        x0, x1 = int(max(0, np.floor(xs.min()))), int(min(W - 1, np.ceil(xs.max())))
        y0, y1 = int(max(0, np.floor(ys.min()))), int(min(H - 1, np.ceil(ys.max())))
        if x1 < x0 or y1 < y0:
            continue
        area = (xs[1] - xs[0]) * (ys[2] - ys[0]) - (xs[2] - xs[0]) * (ys[1] - ys[0])
        if abs(area) < 1e-9:
            continue
        gx, gy = np.meshgrid(np.arange(x0, x1 + 1) + 0.5, np.arange(y0, y1 + 1) + 0.5)
        w0 = ((xs[1] - gx) * (ys[2] - gy) - (xs[2] - gx) * (ys[1] - gy)) / area
        w1 = ((xs[2] - gx) * (ys[0] - gy) - (xs[0] - gx) * (ys[2] - gy)) / area
        w2 = 1 - w0 - w1
        inside = (w0 >= -1e-6) & (w1 >= -1e-6) & (w2 >= -1e-6)
        if not inside.any():
            continue
        dd = w0 * d[ids[0]] + w1 * d[ids[1]] + w2 * d[ids[2]]
        uv = m["uv"][ids]
        u = w0 * uv[0, 0] + w1 * uv[1, 0] + w2 * uv[2, 0]
        v = w0 * uv[0, 1] + w1 * uv[1, 1] + w2 * uv[2, 1]
        tx = np.clip((u * tw).astype(int), 0, tw - 1)
        ty = np.clip((v * th).astype(int), 0, th - 1)
        col = tex[ty, tx]
        sub = depth[y0:y1 + 1, x0:x1 + 1]
        ok = inside & (col[..., 3] >= 128) & (dd > sub)
        sub[ok] = dd[ok]
        rgb = col[..., :3].astype(float) * shade
        if tint is not None:
            rgb *= np.array(tint)
        out = img[y0:y1 + 1, x0:x1 + 1]
        out[ok, :3] = np.clip(rgb[ok], 0, 255).astype(np.uint8)
        out[ok, 3] = 255
    # Liseré sombre autour de la silhouette, pour la lisibilité
    a = img[..., 3] > 0
    edge = np.zeros_like(a)
    edge[1:, :] |= a[1:, :] != a[:-1, :]
    edge[:, 1:] |= a[:, 1:] != a[:, :-1]
    return Image.fromarray(img, "RGBA"), scale


def font(size):
    for f in ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf"):
        try:
            return ImageFont.truetype(f, size)
        except OSError:
            pass
    return ImageFont.load_default()


def board(cells, cols, cell, title=None):
    """Une planche : `cells` = [(image RGBA, légende)]."""
    rows = (len(cells) + cols - 1) // cols
    top = 46 if title else 0
    out = Image.new("RGB", (cols * cell, rows * (cell + 30) + top), BG)
    draw = ImageDraw.Draw(out)
    if title:
        draw.text((14, 10), title, fill=(30, 40, 50), font=font(24))
    for k, (img, label) in enumerate(cells):
        x, y = (k % cols) * cell, (k // cols) * (cell + 30) + top
        out.paste(img, (x, y + 4), img)
        tw = draw.textlength(label, font=font(17))
        draw.text((x + (cell - tw) / 2, y + cell + 2), label, fill=(30, 40, 50), font=font(17))
    return out


if __name__ == "__main__":
    out = sys.argv[1]
    cells = [(render(load(p), 360)[0], pathlib.Path(p).stem) for p in sys.argv[2:]]
    board(cells, min(4, len(cells)), 360).save(out)
