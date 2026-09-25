"""Pixel history along a row of a RenderDoc capture: who draws each pixel, and where.

Run inside qrenderdoc, which carries the renderdoc module (arguments go through the environment):

    set RDC_CAPTURE=build\rdc\coin_frame3928.rdc   & set RDC_ROW=520 & set RDC_XS=610:672:4
    qrenderdoc.exe --ui-python .claude\skills\run-terasology\rdc_row.py

For every pixel and every full-size depth target it prints each fragment of a real draw (100 indices and more):
event, primitive, passed or not, discarded or not, the depth it wrote, the draw's chunkPositionWorld and the
flat grid position (vertexWorldPos) of the three corners of the triangle. The result is written next to the
capture as <capture>.row.txt, then qrenderdoc exits.
"""
import os
import struct
import traceback

import renderdoc as rd

CAP = os.path.abspath(os.environ["RDC_CAPTURE"])
ROW = int(os.environ.get("RDC_ROW", "400"))
start, stop, step = (int(v) for v in os.environ.get("RDC_XS", "600:680:8").split(":"))
XS = list(range(start, stop, step))
OUT = CAP + ".row.txt"
lines = []


def flatten(actions, out):
    for a in actions:
        out.append(a)
        flatten(a.children, out)


def triangle(ctrl, eid, prim, cache):
    if eid not in cache:
        ctrl.SetFrameEvent(eid, True)
        pipe = ctrl.GetPipelineState()
        refl = pipe.GetShaderReflection(rd.ShaderStage.Vertex)
        mf = ctrl.GetPostVSData(0, 0, rd.MeshDataStage.VSOut)
        off, offs = 0, {}
        for o in refl.outputSignature:  # the post-VS data is packed, component by component
            offs[o.varName] = off
            off += 4 * o.compCount
        idx = ctrl.GetBufferData(mf.indexResourceId, mf.indexByteOffset, 0)
        fmt = {1: "B", 2: "H", 4: "I"}[mf.indexByteStride]
        indices = struct.unpack("<%d%s" % (len(idx) // mf.indexByteStride, fmt), idx)
        vb = ctrl.GetBufferData(mf.vertexResourceId, mf.vertexByteOffset, 0)
        chunk = None
        entry = pipe.GetShaderEntryPoint(rd.ShaderStage.Vertex)
        for i in range(len(refl.constantBlocks)):
            for v in ctrl.GetCBufferVariableContents(pipe.GetGraphicsPipelineObject(), refl.resourceId,
                                                     rd.ShaderStage.Vertex, entry, i, rd.ResourceId.Null(), 0, 0):
                if v.name == "chunkPositionWorld":
                    chunk = tuple(round(c, 1) for c in v.value.f32v[:3])
        cache[eid] = (indices, vb, mf.vertexByteStride, offs.get("vertexWorldPos"), chunk)
    indices, vb, stride, off, chunk = cache[eid]
    if off is None or prim * 3 + 2 >= len(indices):
        return None, chunk
    return [tuple(round(c, 1) for c in struct.unpack_from("<3f", vb, vi * stride + off))
            for vi in indices[prim * 3: prim * 3 + 3]], chunk


def work(ctrl):
    acts = []
    flatten(ctrl.GetRootActions(), acts)
    draws = {a.eventId: a for a in acts if a.flags & rd.ActionFlags.Drawcall}
    texs = {t.resourceId: t for t in ctrl.GetTextures()}
    depths = []
    for a in draws.values():
        if a.depthOut != rd.ResourceId.Null() and a.depthOut not in depths:
            depths.append(a.depthOut)
    cache = {}
    for px in XS:
        lines.append("=== x=%d y=%d" % (px, ROW))
        for t in depths:
            tx = texs[t]
            if tx.width < 900:
                continue
            for m in ctrl.PixelHistory(t, int(px * tx.width / 1280), int(ROW * tx.height / 800),
                                       rd.Subresource(0, 0, 0), tx.format.compType):
                a = draws.get(m.eventId)
                if a is None or a.numIndices < 100:
                    continue
                pts, chunk = triangle(ctrl, m.eventId, m.primitiveID, cache)
                lines.append("  depth %s ev %d prim %d passed=%s discard=%s out=%.6f chunk=%s tri=%s" % (
                    str(t).split("::")[1], m.eventId, m.primitiveID, m.Passed(), m.shaderDiscarded,
                    m.shaderOut.depth, chunk, pts))


try:
    pyrenderdoc.LoadCapture(CAP, rd.ReplayOptions(), CAP, False, True)
    pyrenderdoc.Replay().BlockInvoke(work)
except Exception:
    lines.append(traceback.format_exc())
with open(OUT, "w") as f:
    f.write("\n".join(lines))
os._exit(0)
