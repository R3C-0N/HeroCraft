"""Modèles en boîtes, à la manière de Blockbench : des pavés rattachés à des os, peints texel par
texel, exportés en glTF 2.0 à squelette (données embarquées en base64) avec leur texture PNG.

Les dimensions sont en pixels (1/16 de bloc), y vers le haut, la tête vers +z, la gauche de la
bête vers +x, les pieds à y = 0. Chaque face d'un pavé reçoit sa propre case de texture, d'un texel
par pixel (`res` la change pour les grands pavés), peinte d'un motif (`fur`, `stone`, `metal`…) puis
de ses décalcomanies (yeux, runes, coutures…).
"""
import base64
import json
import math
import random
import struct

import numpy as np
from PIL import Image

PIXEL = 1.0 / 16.0
ATLAS_WIDTH = 256

FACES = ("front", "back", "left", "right", "top", "bottom")


# ------------------------------------------------------------------ couleurs


def rgb(hex_color):
    """'#a1b2c3' -> (r, g, b, 255) ; un quatrième octet donne l'alpha."""
    h = hex_color.lstrip("#")
    if len(h) == 3:
        h = "".join(c * 2 for c in h)
    vals = [int(h[i:i + 2], 16) for i in range(0, len(h), 2)]
    if len(vals) == 3:
        vals.append(255)
    return tuple(vals)


def shade(color, k):
    """Éclaircit (k > 0) ou assombrit (k < 0) une couleur."""
    r, g, b, a = color
    if k >= 0:
        return (int(r + (255 - r) * k), int(g + (255 - g) * k), int(b + (255 - b) * k), a)
    return (int(r * (1 + k)), int(g * (1 + k)), int(b * (1 + k)), a)


def mix(c1, c2, t):
    return tuple(int(c1[i] * (1 - t) + c2[i] * t) for i in range(4))


# ------------------------------------------------------------------ motifs


def paint_pattern(img, pattern, base, rng, face):
    """Peint `img` (h × w × 4) d'un motif sur la couleur `base`."""
    h, w = img.shape[:2]
    img[:, :] = base
    if pattern is None or pattern == "flat":
        return
    for y in range(h):
        for x in range(w):
            r = rng.random()
            if pattern == "plain":
                k = (r - 0.5) * 0.10
            elif pattern == "fur":
                # Mèches : des traits verticaux plus sombres ou plus clairs
                k = (r - 0.5) * 0.22 + (0.06 if (x * 7 + y // 2) % 5 == 0 else 0.0) - (0.07 if (x * 3 + y) % 7 == 0 else 0.0)
            elif pattern == "stone":
                k = (r - 0.5) * 0.25
                if (x + 2 * (y // 3)) % 5 == 0 and y % 3 == 0:
                    k -= 0.25
            elif pattern == "metal":
                k = (r - 0.5) * 0.06
                if y == 0 or x == 0:
                    k += 0.18
                if y == h - 1 or x == w - 1:
                    k -= 0.18
            elif pattern == "rivets":
                k = (r - 0.5) * 0.06
                if (x in (1, w - 2) and y in (1, h - 2)) and w > 3 and h > 3:
                    k += 0.35
                elif y == 0 or x == 0:
                    k += 0.14
                elif y == h - 1 or x == w - 1:
                    k -= 0.18
            elif pattern == "wood":
                k = (r - 0.5) * 0.08 + (-0.16 if x % 4 == 0 else 0.0) + (0.05 if (x + y // 5) % 4 == 2 else 0.0)
            elif pattern == "planks":
                k = (r - 0.5) * 0.08 + (-0.22 if y % 4 == 3 else 0.0) + (-0.12 if (x + 5 * (y // 4)) % 9 == 0 else 0.0)
            elif pattern == "bone":
                k = (r - 0.5) * 0.10 + (-0.12 if (y % 6 == 0 and r > 0.4) else 0.0)
            elif pattern == "flesh":
                k = (r - 0.5) * 0.20 + (-0.10 if (x * 5 + y * 3) % 11 == 0 else 0.0)
            elif pattern == "feather":
                # Rangs de plumes : une ligne sombre tous les 3 rangs, décalée en chevrons
                k = (r - 0.5) * 0.08 + (-0.2 if (y + abs((x % 4) - 2)) % 3 == 0 else 0.04)
            elif pattern == "scales":
                k = (r - 0.5) * 0.08 + (-0.18 if (y % 2 == 0 and (x + y // 2) % 2 == 0) else 0.0)
            elif pattern == "crystal":
                k = 0.35 * (1 - (x + y) / max(1, w + h - 2)) - 0.1 + (r - 0.5) * 0.06
            elif pattern == "moss":
                k = (r - 0.5) * 0.3
            elif pattern == "glow":
                k = (r - 0.5) * 0.08
            else:
                raise ValueError(pattern)
            img[y, x] = shade(base, k)


# ------------------------------------------------------------------ géométrie


def rot_matrix(euler):
    """Rotation en degrés [x, y, z], appliquée dans l'ordre z, puis y, puis x (comme Blockbench)."""
    x, y, z = [math.radians(a) for a in euler]
    rx = np.array([[1, 0, 0], [0, math.cos(x), -math.sin(x)], [0, math.sin(x), math.cos(x)]])
    ry = np.array([[math.cos(y), 0, math.sin(y)], [0, 1, 0], [-math.sin(y), 0, math.cos(y)]])
    rz = np.array([[math.cos(z), -math.sin(z), 0], [math.sin(z), math.cos(z), 0], [0, 0, 1]])
    return rx @ ry @ rz


def align_y(direction):
    """Rotation qui envoie +y sur `direction`."""
    d = np.asarray(direction, float)
    d = d / np.linalg.norm(d)
    y = np.array([0.0, 1.0, 0.0])
    v = np.cross(y, d)
    c = float(np.dot(y, d))
    if np.linalg.norm(v) < 1e-9:
        return np.eye(3) if c > 0 else np.diag([1.0, -1.0, -1.0])
    vx = np.array([[0, -v[2], v[1]], [v[2], 0, -v[0]], [-v[1], v[0], 0]])
    return np.eye(3) + vx + vx @ vx * (1 / (1 + c))


class Box:
    def __init__(self, bone, frm, to, color, pattern, decals, res, transform, faces, alpha_faces):
        self.bone = bone
        self.frm = np.array(frm, float)
        self.to = np.array(to, float)
        self.color = rgb(color) if isinstance(color, str) else color
        self.pattern = pattern
        self.decals = decals or {}
        self.res = res
        self.transform = transform  # (matrice 3×3, origine, déplacement)
        self.faces = faces
        self.alpha_faces = alpha_faces


class Model:
    """Un modèle : ses os (nom, parent, pivot) et ses pavés."""

    def __init__(self, ident, seed=None):
        self.ident = ident
        self.bones = {}
        self.order = []
        self.boxes = []
        self.clips = {}
        self.stack = []
        self.rng = random.Random(seed if seed is not None else ident)
        # Taille d'une unité du modèle en blocs (un pixel, 1/16 de bloc, par défaut)
        self.unit = PIXEL

    # -- transformations de groupe : elles s'appliquent aux pavés et aux pivots déclarés dedans
    def push(self, origin=(0, 0, 0), euler=(0, 0, 0), offset=(0, 0, 0), matrix=None):
        m = rot_matrix(euler) if matrix is None else np.asarray(matrix, float)
        self.stack.append((m, np.array(origin, float), np.array(offset, float)))

    def pop(self):
        self.stack.pop()

    def _apply(self, p):
        p = np.array(p, float)
        for m, origin, offset in reversed(self.stack):
            p = m @ (p - origin) + origin + offset
        return p

    def _apply_dir(self, v):
        v = np.array(v, float)
        for m, _, _ in reversed(self.stack):
            v = m @ v
        return v

    def bone(self, name, parent, pivot):
        assert name not in self.bones, name
        assert parent is None or parent in self.bones, parent
        self.bones[name] = (parent, self._apply(pivot))
        self.order.append(name)

    def box(self, bone, frm, to, color, pattern="plain", decals=None, res=1.0, euler=None, origin=None,
            faces=FACES, alpha_faces=False, inflate=0.0):
        """Un pavé de coins `frm` et `to`, tourné de `euler` autour de `origin` (son centre par
        défaut), puis par les groupes ouverts."""
        frm, to = np.minimum(frm, to) - inflate, np.maximum(frm, to) + inflate
        local = np.eye(3)
        o = (np.array(frm) + np.array(to)) / 2 if origin is None else np.array(origin, float)
        if euler is not None:
            local = rot_matrix(euler)
        self.boxes.append(Box(bone, frm, to, color, pattern, decals, res, (local, o, list(self.stack)), faces, alpha_faces))
        return self.boxes[-1]

    def beam(self, bone, start, end, w, d=None, color="#ffffff", pattern="plain", decals=None, res=1.0, twist=0.0):
        """Un pavé tendu de `start` à `end`, de section `w` × `d`."""
        d = w if d is None else d
        start = np.array(start, float)
        end = np.array(end, float)
        length = float(np.linalg.norm(end - start))
        m = align_y(end - start) @ rot_matrix((0, twist, 0))
        frm = start + np.array([-w / 2, 0, -d / 2])
        to = start + np.array([w / 2, length, d / 2])
        self.boxes.append(Box(bone, frm, to, rgb(color) if isinstance(color, str) else color, pattern, decals, res,
                              (m, start, list(self.stack)), FACES, False))
        return self.boxes[-1]

    def mirror(self, fn):
        """Appelle `fn(s, side)` pour les deux côtés : s = −1 à droite (−x), +1 à gauche (+x)."""
        fn(-1, "R")
        fn(1, "L")

    def clip(self, name, duration, channels):
        """Une animation glTF : `channels` = [(os, 'rotation'|'translation'|'scale', [(t, [x, y, z]), …])],
        rotations en degrés, translations en pixels."""
        self.clips[name] = (duration, channels)

    # ---------------------------------------------------------------- export

    def _box_geometry(self, box):
        """Les six faces d'un pavé : coins (bas gauche, bas droit, haut droit, haut gauche) vus de dehors,
        et taille de leur case en texels."""
        (x0, y0, z0), (x1, y1, z1) = box.frm, box.to
        sx, sy, sz = x1 - x0, y1 - y0, z1 - z0
        quads = {
            "front": ([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], sx, sy),
            "back": ([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0], sx, sy),
            "left": ([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1], sz, sy),
            "right": ([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0], sz, sy),
            "top": ([x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0], sx, sz),
            "bottom": ([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], sx, sz),
        }
        local, origin, stack = box.transform
        out = {}
        for face in box.faces:
            *corners, fw, fh = quads[face]
            pts = []
            for c in corners:
                p = local @ (np.array(c, float) - origin) + origin
                for m, o, off in reversed(stack):
                    p = m @ (p - o) + o + off
                pts.append(p)
            tw = max(1, int(round(fw * box.res)))
            th = max(1, int(round(fh * box.res)))
            out[face] = (pts, tw, th)
        return out

    def build(self):
        """Peint les faces, les range dans l'atlas, et renvoie (sommets, indices, image)."""
        faces = []
        for i, box in enumerate(self.boxes):
            geo = self._box_geometry(box)
            for face, (pts, tw, th) in geo.items():
                img = np.zeros((th, tw, 4), np.uint8)
                paint_pattern(img, box.pattern, box.color, self.rng, face)
                for decal in box.decals.get(face, []) + box.decals.get("all", []) + (
                        box.decals.get("sides", []) if face in ("front", "back", "left", "right") else []):
                    apply_decal(img, decal)
                faces.append((box, face, pts, img))
        # Rangement en étagères, des plus hautes aux plus basses
        order = sorted(range(len(faces)), key=lambda k: (-faces[k][3].shape[0], -faces[k][3].shape[1]))
        width = ATLAS_WIDTH
        while max(f[3].shape[1] for f in faces) > width:
            width *= 2
        x = y = shelf = 0
        place = {}
        for k in order:
            h, w = faces[k][3].shape[:2]
            if x + w > width:
                x, y, shelf = 0, y + shelf, 0
            place[k] = (x, y)
            x += w
            shelf = max(shelf, h)
        height = y + shelf
        atlas = np.zeros((height, width, 4), np.uint8)
        for k, (px, py) in place.items():
            img = faces[k][3]
            atlas[py:py + img.shape[0], px:px + img.shape[1]] = img
        vertices = []
        indices = []
        eps = 0.02
        for k, (box, face, pts, img) in enumerate(faces):
            px, py = place[k]
            h, w = img.shape[:2]
            u0, u1 = (px + eps) / width, (px + w - eps) / width
            v0, v1 = (py + eps) / height, (py + h - eps) / height
            uvs = [(u0, v1), (u1, v1), (u1, v0), (u0, v0)]
            n = np.cross(pts[1] - pts[0], pts[3] - pts[0])
            n = n / (np.linalg.norm(n) or 1)
            base = len(vertices)
            for p, uv in zip(pts, uvs):
                vertices.append((p, n, uv, self.order.index(box.bone)))
            indices += [base, base + 1, base + 2, base, base + 2, base + 3]
        return vertices, indices, Image.fromarray(atlas, "RGBA")

    def export(self, gltf_path, png_path):
        vertices, indices, image = self.build()
        image.save(png_path, optimize=True)
        gltf_path.write_text(json.dumps(self.gltf(vertices, indices), separators=(",", ":")), encoding="utf-8")
        return vertices, indices, image

    def gltf(self, vertices, indices):
        blob = bytearray()
        views = []
        accessors = []

        def add(data, fmt, count, comp, kind, minmax=False, target=None):
            while len(blob) % 4:
                blob.append(0)
            offset = len(blob)
            blob.extend(struct.pack("<%d%s" % (len(data), fmt), *data))
            view = {"buffer": 0, "byteOffset": offset, "byteLength": len(blob) - offset}
            if target:
                view["target"] = target
            views.append(view)
            acc = {"bufferView": len(views) - 1, "componentType": comp, "count": count, "type": kind}
            if minmax:
                width = {"SCALAR": 1, "VEC3": 3, "VEC4": 4}[kind]
                cols = [data[i::width] for i in range(width)]
                acc["min"] = [min(c) for c in cols]
                acc["max"] = [max(c) for c in cols]
            accessors.append(acc)
            return len(accessors) - 1

        n = len(vertices)
        pos = [float(c) * self.unit for v in vertices for c in v[0]]
        nor = [float(c) for v in vertices for c in v[1]]
        uv = [float(c) for v in vertices for c in v[2]]
        joints = [c for v in vertices for c in (v[3], 0, 0, 0)]
        weights = [c for v in vertices for c in (1.0, 0.0, 0.0, 0.0)]
        a_pos = add(pos, "f", n, 5126, "VEC3", minmax=True, target=34962)
        a_nor = add(nor, "f", n, 5126, "VEC3", target=34962)
        a_uv = add(uv, "f", n, 5126, "VEC2", target=34962)
        a_j = add(joints, "H", n, 5123, "VEC4", target=34962)
        a_w = add(weights, "f", n, 5126, "VEC4", target=34962)
        a_i = add(indices, "I", len(indices), 5125, "SCALAR", target=34963)

        # Os : nœuds 1…, chacun déplacé de son pivot par rapport à celui de son parent
        nodes = [{"name": self.ident, "mesh": 0, "skin": 0, "children": []}]
        index = {}
        for name in self.order:
            index[name] = len(nodes)
            parent, pivot = self.bones[name]
            ppivot = self.bones[parent][1] if parent else np.zeros(3)
            nodes.append({"name": name, "translation": [round(float(c) * self.unit, 6) for c in pivot - ppivot]})
        for name in self.order:
            parent, _ = self.bones[name]
            holder = nodes[index[parent]] if parent else nodes[0]
            holder.setdefault("children", []).append(index[name])
        ibm = []
        for name in self.order:
            p = self.bones[name][1] * self.unit
            ibm += [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, float(-p[0]), float(-p[1]), float(-p[2]), 1]
        a_ibm = add(ibm, "f", len(self.order), 5126, "MAT4")

        animations = []
        for clip_name, (duration, channels) in self.clips.items():
            samplers, chans = [], []
            for bone, path, keys in channels:
                times = [float(t) for t, _ in keys]
                if path == "rotation":
                    vals = []
                    for _, e in keys:
                        q = quat_from_matrix(rot_matrix(e))
                        vals += q
                    kind = "VEC4"
                elif path == "translation":
                    rest = self.bones[bone][1] - (self.bones[self.bones[bone][0]][1] if self.bones[bone][0] else np.zeros(3))
                    vals = [float(c) for _, v in keys for c in (rest + np.array(v, float)) * self.unit]
                    kind = "VEC3"
                else:
                    vals = [float(c) for _, v in keys for c in v]
                    kind = "VEC3"
                a_t = add(times, "f", len(times), 5126, "SCALAR", minmax=True)
                a_v = add(vals, "f", len(times), 5126, kind)
                samplers.append({"input": a_t, "output": a_v, "interpolation": "LINEAR"})
                chans.append({"sampler": len(samplers) - 1, "target": {"node": index[bone], "path": path}})
            animations.append({"name": clip_name, "samplers": samplers, "channels": chans})

        gltf = {
            "asset": {"version": "2.0", "generator": "HeroCraft tools/modeles_competences"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": nodes,
            "meshes": [{"name": self.ident, "primitives": [{"attributes": {
                "POSITION": a_pos, "NORMAL": a_nor, "TEXCOORD_0": a_uv, "JOINTS_0": a_j, "WEIGHTS_0": a_w},
                "indices": a_i}]}],
            "skins": [{"name": self.ident, "joints": [index[n] for n in self.order], "inverseBindMatrices": a_ibm}],
            "accessors": accessors,
            "bufferViews": views,
            "buffers": [{"byteLength": len(blob), "uri": "data:application/octet-stream;base64," + base64.b64encode(bytes(blob)).decode()}],
        }
        if animations:
            gltf["animations"] = animations
        return gltf


def quat_from_matrix(m):
    """Quaternion [x, y, z, w] d'une matrice de rotation."""
    t = m[0, 0] + m[1, 1] + m[2, 2]
    if t > 0:
        s = math.sqrt(t + 1.0) * 2
        w = 0.25 * s
        x = (m[2, 1] - m[1, 2]) / s
        y = (m[0, 2] - m[2, 0]) / s
        z = (m[1, 0] - m[0, 1]) / s
    elif m[0, 0] > m[1, 1] and m[0, 0] > m[2, 2]:
        s = math.sqrt(1.0 + m[0, 0] - m[1, 1] - m[2, 2]) * 2
        w = (m[2, 1] - m[1, 2]) / s
        x = 0.25 * s
        y = (m[0, 1] + m[1, 0]) / s
        z = (m[0, 2] + m[2, 0]) / s
    elif m[1, 1] > m[2, 2]:
        s = math.sqrt(1.0 + m[1, 1] - m[0, 0] - m[2, 2]) * 2
        w = (m[0, 2] - m[2, 0]) / s
        x = (m[0, 1] + m[1, 0]) / s
        y = 0.25 * s
        z = (m[1, 2] + m[2, 1]) / s
    else:
        s = math.sqrt(1.0 + m[2, 2] - m[0, 0] - m[1, 1]) * 2
        w = (m[1, 0] - m[0, 1]) / s
        x = (m[0, 2] + m[2, 0]) / s
        y = (m[1, 2] + m[2, 1]) / s
        z = 0.25 * s
    return [float(x), float(y), float(z), float(w)]


# ------------------------------------------------------------------ décalcomanies


def apply_decal(img, decal):
    """Une décalcomanie sur une face (coordonnées du haut gauche de la face vue de dehors) :
    ('px', x, y, couleur), ('rect', x, y, w, h, couleur), ('row', y, couleur), ('col', x, couleur),
    ('rows', [y…], couleur), ('pixels', [(x, y)…], couleur), ('frame', couleur) ; un x ou y négatif
    se compte depuis l'autre bord, et ('mirror', décalcomanie) la pose aussi en miroir."""
    h, w = img.shape[:2]
    kind = decal[0]

    def put(x, y, c):
        x = x + w if x < 0 else x
        y = y + h if y < 0 else y
        if 0 <= x < w and 0 <= y < h:
            img[y, x] = rgb(c) if isinstance(c, str) else c

    if kind == "px":
        put(decal[1], decal[2], decal[3])
    elif kind == "rect":
        _, x, y, rw, rh, c = decal
        x = x + w if x < 0 else x
        y = y + h if y < 0 else y
        for j in range(rh):
            for i in range(rw):
                put(x + i, y + j, c)
    elif kind == "row":
        for i in range(w):
            put(i, decal[1], decal[2])
    elif kind == "col":
        for j in range(h):
            put(decal[1], j, decal[2])
    elif kind == "rows":
        for y in decal[1]:
            for i in range(w):
                put(i, y, decal[2])
    elif kind == "pixels":
        for x, y in decal[1]:
            put(x, y, decal[2])
    elif kind == "frame":
        for i in range(w):
            put(i, 0, decal[1])
            put(i, h - 1, decal[1])
        for j in range(h):
            put(0, j, decal[1])
            put(w - 1, j, decal[1])
    elif kind == "mirror":
        inner = decal[1]
        apply_decal(img, inner)
        # Puis la même à la colonne miroir
        if inner[0] == "px":
            put(w - 1 - inner[1], inner[2], inner[3])
        elif inner[0] == "rect":
            _, x, y, rw, rh, c = inner
            for j in range(rh):
                for i in range(rw):
                    put(w - 1 - (x + i), y + j, c)
        elif inner[0] == "pixels":
            for x, y in inner[1]:
                put(w - 1 - x, y, inner[2])
    elif kind == "cut":
        # Texels transparents : la pièce y est découpée
        for x, y in decal[1]:
            put(x, y, (0, 0, 0, 0))
    elif kind == "fn":
        decal[1](img)
    else:
        raise ValueError(kind)
