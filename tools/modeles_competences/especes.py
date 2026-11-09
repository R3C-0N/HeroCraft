"""Fiches des créatures des compétences (`assets/creatures/*.json`) : formes du Druide et
invocations du Nécromancien. Elles n'ont pas de champ `spawn` : elles n'apparaissent jamais
d'elles-mêmes, seule une compétence les fait venir.
"""


def quadruped(foulee=0.6, amp=24, run=1.4, head="head", tail="tail", extra=None):
    """Animations d'un quadrupède aux os `legFR`, `legFL`, `legBR`, `legBL`, `head`, `tail`."""
    legs = lambda a, ph=(0, 0.5, 0.5, 0): {  # noqa: E731
        "legFR": {"rotation": [a, 0, 0], "phase": ph[0]},
        "legFL": {"rotation": [a, 0, 0], "phase": ph[1]},
        "legBR": {"rotation": [a, 0, 0], "phase": ph[2]},
        "legBL": {"rotation": [a, 0, 0], "phase": ph[3]},
    }
    t = {tail: None} if tail else {}
    anims = {
        "repos": {"cycle": 3.5, "pause": {"poids": 2, "duree": [2, 5]}, "os": {
            head: {"rotation": [2, 0, 0]}, "root": {"translation": [0, 0.006, 0], "frequence": 2},
            **({tail: {"rotation": [0, 8, 0], "frequence": 2}} if t else {})}},
        "marche": {"foulee": foulee, "os": {
            **legs(amp), "root": {"translation": [0, 0.015, 0], "phase": 0.25, "frequence": 2},
            head: {"rotation": [3, 0, 0], "phase": 0.1, "frequence": 2},
            **({tail: {"rotation": [0, 10, 0]}} if t else {})}},
        "course": {"foulee": foulee * run * 1.6, "os": {
            **legs(amp * 1.5, (0, 0.1, 0.5, 0.6)), "root": {"rotation": [5, 0, 0], "translation": [0, 0.04, 0], "phase": 0.3},
            head: {"rotation": [6, 0, 0], "rotation_fixe": [8, 0, 0], "phase": 0.55},
            **({tail: {"rotation": [8, 0, 0], "rotation_fixe": [20, 0, 0], "phase": 0.3}} if t else {})}},
        "attaque": {"cycle": 0.8, "os": {
            "root": {"rotation": [-7, 0, 0], "translation": [0, 0.03, 0.1], "phase": 0.75},
            head: {"rotation": [14, 0, 0], "rotation_fixe": [-8, 0, 0], "phase": 0.1, "frequence": 2},
            "legFR": {"rotation": [-25, 0, 0], "phase": 0.75}, "legFL": {"rotation": [-25, 0, 0], "phase": 0.8}}},
        "coup": {"os": {
            "root": {"rotation": [-9, 0, 0], "frequence": 0.5}, head: {"rotation": [-14, 0, 0], "frequence": 0.5},
            **({tail: {"rotation": [25, 0, 0], "frequence": 0.5}} if t else {})}},
    }
    for k, v in (extra or {}).items():
        anims[k] = v
    return anims


def winged(fold=80, flap=35):
    """Ailes `wingR`/`wingL` : repliées vers l'arrière au repos, battantes en marche et en course."""
    folded = {"wingR": {"rotation_fixe": [0, -fold, 0]}, "wingL": {"rotation_fixe": [0, fold, 0]}}
    flapping = lambda a, f=1: {"wingR": {"rotation": [0, 0, -a], "frequence": f}, "wingL": {"rotation": [0, 0, a], "frequence": f}}  # noqa: E731
    return folded, flapping


def biped(foulee=0.9, amp=26):
    """Animations d'un bipède aux os `legR`, `legL`, `armR`, `armL`, `head` (et `chest`)."""
    return {
        "repos": {"cycle": 4, "pause": {"poids": 2, "duree": [2, 5]}, "os": {
            "chest": {"rotation": [3, 0, 0]}, "head": {"rotation": [0, 12, 0], "frequence": 0.5},
            "armR": {"rotation": [4, 0, 2]}, "armL": {"rotation": [4, 0, -2], "phase": 0.5}}},
        "marche": {"foulee": foulee, "os": {
            "legR": {"rotation": [amp, 0, 0]}, "legL": {"rotation": [amp, 0, 0], "phase": 0.5},
            "armR": {"rotation": [amp * 0.8, 0, 0], "phase": 0.5}, "armL": {"rotation": [amp * 0.8, 0, 0]},
            "root": {"translation": [0, 0.02, 0], "phase": 0.25, "frequence": 2}}},
        "course": {"foulee": foulee * 1.8, "os": {
            "legR": {"rotation": [amp * 1.5, 0, 0]}, "legL": {"rotation": [amp * 1.5, 0, 0], "phase": 0.5},
            "armR": {"rotation": [amp * 1.2, 0, 0], "phase": 0.5}, "armL": {"rotation": [amp * 1.2, 0, 0]},
            "chest": {"rotation_fixe": [10, 0, 0]},
            "root": {"translation": [0, 0.05, 0], "phase": 0.25, "frequence": 2}}},
        "attaque": {"cycle": 0.9, "os": {
            "armR": {"rotation": [-70, 0, 0], "rotation_fixe": [-40, 0, 0]},
            "armL": {"rotation": [-70, 0, 0], "rotation_fixe": [-40, 0, 0], "phase": 0.5},
            "chest": {"rotation": [8, 0, 0], "frequence": 2}}},
        "coup": {"os": {"chest": {"rotation": [-12, 0, 0], "frequence": 0.5}, "head": {"rotation": [-15, 0, 0], "frequence": 0.5}}},
    }


def species():
    folded, flapping = winged()
    bird = {
        "repos": {"cycle": 3, "pause": {"poids": 2, "duree": [2, 5]}, "os": {
            **folded, "head": {"rotation": [0, 25, 0], "frequence": 0.5}, "tail": {"rotation": [6, 0, 0], "frequence": 2}}},
        "marche": {"cycle": 0.6, "os": {**flapping(30), "root": {"translation": [0, 0.02, 0], "phase": 0.5},
                                        "tail": {"rotation": [5, 0, 0]}, "legR": {"rotation_fixe": [-60, 0, 0]}, "legL": {"rotation_fixe": [-60, 0, 0]}}},
        "course": {"cycle": 0.4, "os": {**flapping(42), "root": {"rotation_fixe": [10, 0, 0], "translation": [0, 0.03, 0], "phase": 0.5},
                                        "legR": {"rotation_fixe": [-70, 0, 0]}, "legL": {"rotation_fixe": [-70, 0, 0]}}},
        "attaque": {"cycle": 0.6, "os": {**flapping(50), "legR": {"rotation": [-40, 0, 0], "rotation_fixe": [-30, 0, 0]},
                                         "legL": {"rotation": [-40, 0, 0], "rotation_fixe": [-30, 0, 0]}, "head": {"rotation": [20, 0, 0], "frequence": 2}}},
        "coup": {"os": {"root": {"rotation": [-15, 0, 0], "frequence": 0.5}, **flapping(30, 0.5)}},
    }
    winged_quadruped = lambda foulee, amp: quadruped(foulee, amp, extra=None)  # noqa: E731

    griffin = winged_quadruped(0.7, 26)
    for name in ("repos", "marche"):
        griffin[name]["os"].update({"wingR": {"rotation": [0, 0, -4], "frequence": 0.5}, "wingL": {"rotation": [0, 0, 4], "frequence": 0.5}})
    griffin["course"]["os"].update(flapping(40, 2))
    griffin["attaque"]["os"].update(flapping(45, 2))
    chimera = winged_quadruped(1.2, 22)
    chimera["course"]["os"].update(flapping(30, 1))
    chimera["attaque"]["os"].update(flapping(35, 2))
    chimera["rugit"] = {"cycle": 2.5, "pause": {"poids": 0.5, "duree": [2, 4]}, "os": {
        "head": {"rotation": [4, 0, 0], "rotation_fixe": [-30, 0, 0], "frequence": 4},
        "root": {"rotation_fixe": [-6, 0, 0]}, **{k: {"rotation_fixe": [0, 0, v]} for k, v in (("wingR", -40), ("wingL", 40))}}}

    wolf = quadruped(0.6, 24, extra={"hurle": {"cycle": 2.5, "pause": {"poids": 0.4, "duree": [3, 5]}, "os": {
        "root": {"rotation_fixe": [-10, 0, 0]}, "head": {"rotation": [3, 0, 0], "rotation_fixe": [-50, 0, 0], "frequence": 4},
        "tail": {"rotation_fixe": [-15, 0, 0]}}}})
    bear_roar = {"cycle": 2.0, "pause": {"poids": 0.5, "duree": [2, 3]}, "os": {
        "root": {"rotation_fixe": [-30, 0, 0], "translation_fixe": [0, 0.15, -0.1]},
        "legFR": {"rotation": [10, 0, 0], "rotation_fixe": [-60, 0, 0], "frequence": 2},
        "legFL": {"rotation": [10, 0, 0], "rotation_fixe": [-60, 0, 0], "frequence": 2, "phase": 0.5},
        "legBR": {"rotation_fixe": [30, 0, 0]}, "legBL": {"rotation_fixe": [30, 0, 0]},
        "head": {"rotation": [3, 0, 0], "rotation_fixe": [-15, 0, 0], "frequence": 6}}}

    return {
        "forestWolf": {
            "name": "Loup des Bois", "description": "Forme de loup du Druide : rapide, crinière de feuilles et yeux de sève.",
            "body": {"half_width": 0.3, "half_length": 1.0, "height": 1.3}, "walk_speed": 2.2, "health": 30,
            "model": wolf},
        "runicBear": {
            "name": "Ours Runique", "description": "Forme d'ours du Druide, épaules de pierre gravées de runes vivantes.",
            "body": {"half_width": 0.55, "half_length": 1.3, "height": 1.85}, "walk_speed": 1.5, "health": 60,
            "model": quadruped(0.75, 20, extra={"rugit": bear_roar})},
        "ancientStag": {
            "name": "Grand Cerf Ancien", "description": "Forme de cerf du Druide, aux bois immenses où pousse la mousse.",
            "body": {"half_width": 0.35, "half_length": 0.9, "height": 2.4}, "walk_speed": 2.0, "health": 40,
            "model": quadruped(0.8, 22)},
        "peregrineFalcon": {
            "name": "Faucon Pèlerin", "description": "Forme de faucon du Druide, taillée pour le vol.",
            "body": {"half_width": 0.3, "half_length": 0.45, "height": 0.9}, "walk_speed": 3.0, "health": 15,
            "model": bird},
        "owlbear": {
            "name": "Ours-Hibou", "description": "Forme hybride du Druide : la masse de l'ours, la tête et les serres du hibou.",
            "body": {"half_width": 0.55, "half_length": 1.2, "height": 2.0}, "walk_speed": 1.6, "health": 70,
            "model": quadruped(0.75, 22, extra={"rugit": bear_roar})},
        "wendigo": {
            "name": "Wendigo", "description": "Forme hybride du Druide : un être décharné aux bois noircis, environné de brume.",
            "body": {"half_width": 0.4, "half_length": 0.4, "height": 3.1}, "walk_speed": 2.0, "health": 50,
            "model": biped(1.1, 24)},
        "sylvanGriffin": {
            "name": "Griffon Sylvestre", "description": "Forme hybride du Druide : avant d'aigle, arrière de fauve, ailes feuillues.",
            "body": {"half_width": 0.45, "half_length": 1.0, "height": 2.0}, "walk_speed": 2.4, "health": 45,
            "model": griffin},
        "voidChimera": {
            "name": "Chimère du Néant", "description": "Forme colossale de l'Avatar du Néant, constellée de cristaux d'Étherium.",
            "body": {"half_width": 0.9, "half_length": 2.0, "height": 3.4}, "walk_speed": 1.8, "health": 150,
            "model": chimera},
    }
