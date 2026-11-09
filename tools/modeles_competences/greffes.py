"""Les transformations partielles du Druide : des pièces greffées sur le corps du héros.

Chaque pièce est modelée sur le héros humain par défaut, dans son repère au repos (pieds à zéro,
face vers +z, sa gauche vers +x), en texels du patron (1 texel = 0,05625 bloc, la taille d'un héros
de 1,8 bloc). Ses os portent les noms des os du héros (`leftForearm`, `torso`…, voir
`Bone::name` dans `crates/client/src/hero_model.rs`) : un sommet de l'os `B` suit `B` comme la
figure, `figure.bone_frames(pose)[B].apply(p / 0.05625)`.
"""
from boites import Model

HERO_TEXEL = 0.05625

# Pivots des os du héros humain par défaut (texels), comme `hero_model::figure`
PIVOTS = {
    "body": ((0, 0, 0), None),
    "torso": ((0, 12, 0), "body"),
    "head": ((0, 24, 0), "torso"),
    "rightArm": ((-6, 22, 0), "torso"),
    "leftArm": ((6, 22, 0), "torso"),
    "rightForearm": ((-6, 19, 0), "rightArm"),
    "leftForearm": ((6, 19, 0), "leftArm"),
    "rightHand": ((-6, 15, 0), "rightForearm"),
    "leftHand": ((6, 15, 0), "leftForearm"),
    "rightLeg": ((-2, 12, 0), "body"),
    "leftLeg": ((2, 12, 0), "body"),
    "rightShin": ((-2, 6, 0), "rightLeg"),
    "leftShin": ((2, 6, 0), "leftLeg"),
}


def graft(ident, bones):
    m = Model(ident)
    m.unit = HERO_TEXEL
    wanted = set()
    for b in bones:
        while b:
            wanted.add(b)
            b = PIVOTS[b][1]
    for name, (pivot, parent) in PIVOTS.items():
        if name in wanted:
            m.bone(name, parent, pivot)
    return m


def wolf_paw():
    """Griffes de la Meute : l'avant-bras gauche se couvre de fourrure, la main devient une patte
    griffue. (Pour le bras droit, prendre x → −x et `right…`.)"""
    m = graft("wolfPaw", ["leftArm", "leftForearm", "leftHand"])
    fur, dark, pad, claw = "#6e6457", "#4a433b", "#2a2420", "#e6dfcf"
    m.box("leftArm", (3.6, 19, -2.4), (8.4, 21.5, 2.4), fur, "fur", {"sides": [("cut", [(x, -1) for x in range(0, 6, 2)])]})
    m.box("leftForearm", (3.4, 15, -2.6), (8.6, 19.5, 2.6), fur, "fur", {"front": [("px", 2, 1, "#4f8a2e"), ("px", 3, 3, "#4f8a2e")]})
    m.box("leftHand", (3.0, 10.5, -3.0), (9.0, 15.2, 3.4), fur, "fur", {
        "bottom": [("rect", 0, 0, 99, 99, pad), ("rect", 2, 2, 2, 2, "#3d332c")], "front": [("row", -1, dark)]})
    for k in range(4):
        x = 3.8 + k * 1.45
        m.beam("leftHand", (x, 11, 2.6), (x, 8.6, 4.2), 0.7, color=claw, pattern="bone")
    m.beam("leftHand", (3.2, 12, 1.5), (2.2, 10.5, 2.6), 0.7, color=claw, pattern="bone")
    return m


def bear_hide():
    """Échine du Plantigrade : une bosse de fourrure d'ours sur le dos, un mantelet sur les
    épaules."""
    m = graft("bearHide", ["torso", "rightArm", "leftArm"])
    fur, dark = "#5a3d28", "#3d2a1c"
    m.box("torso", (-5, 15, -4.8), (5, 25, -1.6), fur, "fur", {"back": [("row", -1, dark)]})
    m.box("torso", (-4, 21, -5.8), (4, 25.5, -4.6), fur, "fur")
    m.box("torso", (-5.2, 22.5, -2.6), (5.2, 25.6, 2.6), fur, "fur", {"front": [("cut", [(x, -1) for x in range(0, 10, 2)])]})
    for name, s in (("rightArm", -1), ("leftArm", 1)):
        m.box(name, (s * 6 - 2.6, 19.5, -2.6), (s * 6 + 2.6, 25, 2.6), fur, "fur", {"sides": [("cut", [(x, -1) for x in range(1, 5, 2)])]})
    return m


def elk_legs():
    """Jambes de l'Élan : cuisses fourrées, canons fins et sabots fendus."""
    m = graft("elkLegs", ["rightLeg", "leftLeg", "rightShin", "leftShin"])
    coat, dark, hoof = "#9a6a40", "#6e4a2a", "#231a14"
    for leg, shin, s in (("rightLeg", "rightShin", -1), ("leftLeg", "leftShin", 1)):
        x = s * 2
        m.box(leg, (x - 2.4, 6, -2.5), (x + 2.4, 12.3, 2.5), coat, "fur", {"front": [("rect", 1, 0, 3, 2, "#c8a878")]})
        m.box(shin, (x - 2.25, 2, -2.25), (x + 2.25, 6.2, 2.25), dark, "fur")
        m.box(shin, (x - 2.5, 0, -2.4), (x + 2.5, 2.2, 3.2), hoof, "plain", {"front": [("col", 2, "#0e0a08")], "top": [("col", 2, "#0e0a08")]})
        m.box(shin, (x - 1, 1.6, -3.2), (x + 1, 3, -2.2), hoof, "plain")
    return m


def gale_feathers():
    """Plumes de Bourrasque : des rangs de plumes le long de chaque bras ; bras levés, ce sont des
    ailes."""
    m = graft("galeFeathers", ["rightArm", "leftArm", "rightForearm", "leftForearm", "rightHand", "leftHand"])
    light, mid, tip = "#d8d2c2", "#9aa4ad", "#4e5a6a"
    for side, s in (("right", -1), ("left", 1)):
        x = s * 6
        for bone, y0, y1, depth, c in ((side + "Arm", 19, 24, 5, light), (side + "Forearm", 15, 19, 6.5, mid), (side + "Hand", 9, 15, 8, tip)):
            cut = [(z, y) for z in range(int(depth)) for y in range(int(y1 - y0)) if z >= depth - 1 and y % 2 == 1]
            m.box(bone, (x - 0.4, y0, -2 - depth), (x + 0.4, y1, -1.5), c, "feather",
                  {"left": [("cut", cut), ("row", -1, tip)], "right": [("cut", [(int(depth) - 1 - z, y) for z, y in cut]), ("row", -1, tip)]})
    return m


ALL = [wolf_paw, bear_hide, elk_legs, gale_feathers]
