#!/usr/bin/env python3
"""Measure the renderer on three fixed scenes, the same way every time.

A before/after needs the same view, the same settings and the same moment in
the loading of the world. This script owns all three: it launches the game on
the newest save with the inspection port open, waits until the distant terrain
has nothing left to build, then measures

    land    standing on the ground, looking along it
    sea     above the sea, looking out over the water
    flight  a straight flight at ghost speed, over ground that is still loading

For land and sea: debug:gpuTimes (per render pass), debug:frameTimes, the
render line of /stats and distantStats, plus a screenshot at the same framing.
For flight: debug:frameTimes over the whole flight, which is where the loading
stutter shows.

The dynamic shadow map node is only created when the world loads, so turning
shadows on at runtime measures nothing. --shadows therefore relaunches with
dynamicShadows set in config.cfg, and puts the file back afterwards. And the
node only redraws when the camera or the sun has moved: standing still it costs
nothing, so its price shows in the flight, not in the still scenes.

    python .claude/skills/run-terasology/bench.py --label lot1
    python .claude/skills/run-terasology/bench.py --label lot1 --shadows
    python .claude/skills/run-terasology/bench.py --label lot1 --only land,sea

Results go to build/bench/<label>[-shadows].txt and the screenshots to
build/shots/bench-<label>-<scene>.png. The character is put back on its feet
where the scenes start, in walking mode, before the game is closed.
"""
import argparse
import json
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

SKILL_DIR = Path(__file__).resolve().parent
ROOT = SKILL_DIR.parents[2]
DRIVER = SKILL_DIR / "driver.py"
CONFIG = ROOT / "config.cfg"
PORT = 17888
BASE = "http://127.0.0.1:%d" % PORT

# Where each scene starts: position, yaw, pitch (pitch positive downwards).
# Taken from the measurements of 25 September 2026 on the save `main`.
HOME = (4704, 41.5, 4973)
SCENES = {
    "land": {"pos": HOME, "look": (45, -5)},
    "sea": {"pos": (4968.5, 69.2, 4988.5), "look": (135, 5)},
    "flight": {"pos": (4844, 60, 5113), "look": (135, -3), "seconds": 20},
}
GPU_FRAMES = 300


def http(path, data=None, timeout=10):
    req = urllib.request.Request(BASE + path, data=data.encode() if data is not None else None,
                                 method="POST" if data is not None else "GET")
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read().decode("utf-8", errors="replace")


def console(command, retries=5):
    """Under load the port can answer 503 without running the command: retry until it runs."""
    for _ in range(retries):
        try:
            out = http("/console", command)
            if not out.startswith("error="):
                return out.strip()
        except urllib.error.HTTPError as e:
            if e.code != 503:
                raise
        time.sleep(1)
    raise RuntimeError("the console never ran: " + command)


def driver(*args):
    return subprocess.run([sys.executable, str(DRIVER), *args], cwd=str(ROOT)).returncode


def newest_log():
    runs = [p for p in (ROOT / "logs").iterdir() if p.is_dir() and p.name[0].isdigit()]
    run = max(runs, key=lambda p: p.stat().st_mtime)
    return max(run.glob("Terasology-Game*.log"), key=lambda p: p.stat().st_mtime)


def wait_log(pattern, timeout):
    """The reports arrive later, as console messages mirrored into the log."""
    rx = re.compile(pattern)
    deadline = time.time() + timeout
    while time.time() < deadline:
        lines = [l for l in newest_log().read_text(encoding="utf-8", errors="replace").splitlines()
                 if rx.search(l)]
        if lines:
            time.sleep(1)  # a multi-line report is written line after line
            lines = [l for l in newest_log().read_text(encoding="utf-8", errors="replace").splitlines()
                     if rx.search(l)]
            return [re.sub(r"^.*?\] ", "", l) for l in lines]
        time.sleep(1)
    return ["(no report within %d s: %s)" % (timeout, pattern)]


def wait_port(timeout=240):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            if re.search(r"visible_chunks=[1-9]", http("/stats", timeout=3)):
                return
        except OSError:
            pass
        time.sleep(3)
    raise RuntimeError("the world never showed a chunk")


def wait_distant(timeout=300, still=3):
    """Measure only once the distant terrain has stopped building: a node in flight costs CPU.

    "pending=0" alone is not enough: right after a load or a teleport the selection has not yet
    seen the new eye, the queue is empty for a moment, and 25 nodes of an eventual 80 were
    measured as if they were the scene. What counts is the build counter standing still for
    `still` readings in a row, five seconds apart.
    """
    deadline = time.time() + timeout
    last, previous, steady = "", None, 0
    while time.time() < deadline:
        last = console("distantStats")
        m = re.search(r"nodes=(\d+) .*built=(\d+)", last)
        now = m.groups() if m else None
        steady = steady + 1 if now is not None and now == previous and "stale=0" in last else 0
        if steady >= still:
            return last
        previous = now
        time.sleep(5)
    return last + "  (still building after %d s)" % timeout


def player_mode():
    m = re.search(r'"mode":"([A-Z_]+)"', http("/entity/player"))
    return m.group(1) if m else "?"


def set_ghost(on):
    if (player_mode() == "GHOSTING") != on:
        console("ghost")
        time.sleep(1)


def place(scene):
    x, y, z = scene["pos"]
    console("teleport %s %s %s" % (x, y, z))
    console("look %s %s" % scene["look"])


def forwards_key():
    """config.cfg names the bind by its US physical position, which is what driver.py sends."""
    binds = json.loads(CONFIG.read_text(encoding="utf-8")).get("binds", {})
    for module in binds.values():
        for name, keys in module.items():
            if name.endswith("forwards") and keys:
                return keys[0].replace("key_", "") if isinstance(keys, list) else keys.replace("key_", "")
    return "w"


def measure_still(name, scene, tag, out):
    place(scene)
    time.sleep(8)  # let the chunks around the new spot mesh
    out.append("## %s  pos=%s look=%s" % (name, scene["pos"], scene["look"]))
    out.append("settled: " + wait_distant())
    out.append(http("/view").strip())
    label = "%s-%s" % (tag, name)
    console("debug:gpuTimes 2 %d %s" % (GPU_FRAMES, label))
    out += wait_log(r"gpuTimes %s\b" % re.escape(label), 60)
    console("debug:frameTimes 1 8 %s" % label)
    out += wait_log(r"frameTimes %s\b" % re.escape(label), 30)
    out += [l for l in http("/stats").splitlines() if l.startswith(("fps", "render", "chunks"))]
    out.append(console("distantStats"))
    driver("shot", "bench-%s.png" % label, "--delay", "0.5")
    out.append("")


def measure_flight(scene, tag, out):
    place(scene)
    time.sleep(8)
    wait_distant()
    label = "%s-flight" % tag
    out.append("## flight  from pos=%s look=%s, %d s" % (scene["pos"], scene["look"], scene["seconds"]))
    console("debug:frameTimes 2 %d %s" % (scene["seconds"], label))
    driver("hold", "shift", forwards_key(), "--ms", str((scene["seconds"] + 3) * 1000))
    out += wait_log(r"frameTimes %s\b" % re.escape(label), 30)
    out.append(http("/view").strip())
    out.append(console("distantStats"))
    out.append("")


def run(tag, only, shadows):
    backup = CONFIG.read_text(encoding="utf-8")
    cfg = json.loads(backup)
    rendering = cfg["rendering"]
    if shadows:
        rendering["dynamicShadows"] = True
        CONFIG.write_text(json.dumps(cfg, indent=2), encoding="utf-8")
    out = ["# bench %s  %s  shadows=%s" % (tag, time.strftime("%Y-%m-%d %H:%M"), shadows),
           "viewDistance=%s chunkLods=%s fboScale=%s animateWater=%s" % (
               rendering.get("viewDistance"), rendering.get("chunkLods"), rendering.get("fboScale"),
               rendering.get("animateWater")), ""]
    try:
        if driver("launch", "--load-last-game", "--",
                  "--inspect-port=%d" % PORT, "--inspect-allow-console") != 0:
            raise RuntimeError("launch failed")
        wait_port()
        set_ghost(True)
        console("debug:renderingSetting frameLimit 0")
        console("debug:renderingSetting vSync false")
        place(SCENES["land"])
        out.append("distant settled: " + wait_distant())
        out.append("")
        for name in only:
            if name == "flight":
                measure_flight(SCENES[name], tag, out)
            else:
                measure_still(name, SCENES[name], tag, out)
    finally:
        try:
            console("teleport %s %s %s" % HOME)
            time.sleep(2)
            set_ghost(False)
            console("debug:renderingSetting frameLimit %s" % rendering.get("frameLimit", 60))
        except Exception as e:  # the game may be gone; the config still has to come back
            out.append("restore failed: %s" % e)
        driver("quit")
        CONFIG.write_text(backup, encoding="utf-8")
    return out


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--label", required=True, help="names the result file, e.g. baseline or lot1")
    p.add_argument("--shadows", action="store_true", help="relaunch with dynamic shadows on")
    p.add_argument("--only", default="land,sea,flight", help="comma-separated scenes")
    a = p.parse_args()
    only = [s for s in a.only.split(",") if s]
    for s in only:
        if s not in SCENES:
            p.error("unknown scene %s (known: %s)" % (s, ", ".join(SCENES)))
    tag = a.label + ("-shadows" if a.shadows else "")
    out = run(tag, only, a.shadows)
    dest = ROOT / "build" / "bench" / (tag + ".txt")
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text("\n".join(out) + "\n", encoding="utf-8")
    print("\n".join(out))
    print("-> %s" % dest)


if __name__ == "__main__":
    main()
