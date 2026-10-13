// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocraft.ui.legende;

import org.joml.Vector3f;
import org.terasology.engine.rendering.assets.mesh.Mesh;
import org.terasology.engine.rendering.assets.mesh.MeshBuilder;
import org.terasology.engine.rendering.assets.mesh.StandardMeshData;
import org.terasology.engine.utilities.Assets;
import org.terasology.nui.Color;

import java.util.ArrayList;
import java.util.List;

/**
 * The hero's body as a static mesh, for the creator's preview: the boxes and the two hair plates of
 * {@code tools/heros/construire.py}, at rest, in pattern units (a hero is 32 high), feet at zero, facing +z.
 * <p>
 * It mirrors the skeletal mesh rather than reading it, because a {@code SkeletalMesh} asset does not hand its
 * vertices back. If the two ever drift apart, the script is the reference.
 * <p>
 * Layer 1 comes first in the index buffer and layer 2 after it: the UI draws with blending, so a transparent
 * texel of layer 2 must find layer 1 already drawn behind it.
 * <p>
 * The builder gives neither normals nor colours, and the preview's shader needs both: they are added once the
 * positions are in, one normal per corner.
 */
final class Figure {

    private static final float SHEET = 64f;
    private static final float MARGIN = 0.01f;

    private Figure() {
    }

    /** @param largeur how much wider than drawn, across and through: the build */
    static Mesh build(float largeur) {
        Pinceau pinceau = new Pinceau();
        // layer 1 — centre x, bottom, size, pattern corner
        box(pinceau, 0, 24, 8, 8, 8, 0, 0, 0);
        box(pinceau, 0, 12, 8, 12, 4, 16, 16, 0);
        box(pinceau, -6, 12, 4, 12, 4, 40, 16, 0);
        box(pinceau, 6, 12, 4, 12, 4, 32, 48, 0);
        box(pinceau, -2, 0, 4, 12, 4, 0, 16, 0);
        box(pinceau, 2, 0, 4, 12, 4, 16, 48, 0);
        // layer 2
        box(pinceau, 0, 24, 8, 8, 8, 32, 0, 0.5f);
        box(pinceau, 0, 12, 8, 12, 4, 16, 32, 0.25f);
        box(pinceau, -6, 12, 4, 12, 4, 40, 32, 0.25f);
        box(pinceau, 6, 12, 4, 12, 4, 48, 48, 0.25f);
        box(pinceau, -2, 0, 4, 12, 4, 0, 32, 0.25f);
        box(pinceau, 2, 0, 4, 12, 4, 0, 48, 0.25f);
        // beard and hanging hair
        plate(pinceau, 20, 4.05f, 4.45f, 0, 0);
        plate(pinceau, 20, -4.45f, -4.05f, 24, 0);

        StandardMeshData data = pinceau.mesh.buildMeshData();
        for (int i = 0; i < data.position.elements(); i++) {
            data.position.set(i, data.position.get(i, new Vector3f()).mul(largeur, 1, largeur));
        }
        for (Vector3f normal : pinceau.normals) {
            data.normal.put(normal);
            data.color0.put(Color.WHITE);
        }
        return Assets.generateAsset(data, Mesh.class);
    }

    /** A box unwrapped the Minecraft way from its pattern corner, inflated by {@code g} on every side. */
    private static void box(Pinceau b, float cx, float bottom, int w, int h, int d, int u, int v, float g) {
        float x0 = cx - w / 2f - g;
        float x1 = cx + w / 2f + g;
        float y0 = bottom - g;
        float y1 = bottom + h + g;
        float z0 = -d / 2f - g;
        float z1 = d / 2f + g;
        quadZ(b, x0, x1, y0, y1, z1, true, u + d, v + d, w, h);
        quadZ(b, x0, x1, y0, y1, z0, false, u + d + w + d, v + d, w, h);
        quadX(b, z0, z1, y0, y1, x1, true, u + d + w, v + d, d, h);
        quadX(b, z0, z1, y0, y1, x0, false, u, v + d, d, h);
        quadY(b, x0, x1, z0, z1, y1, true, u + d, v, w, d);
        quadY(b, x0, x1, z0, z1, y0, false, u + d + w, v, w, d);
    }

    /** An 8 × 8 plate from the shoulders up, both broad faces reading the whole square, its edges the border. */
    private static void plate(Pinceau b, float bottom, float z0, float z1, int u, int v) {
        float x0 = -4;
        float x1 = 4;
        float y0 = bottom;
        float y1 = bottom + 8;
        quadZ(b, x0, x1, y0, y1, z1, true, u, v, 8, 8);
        quadZ(b, x0, x1, y0, y1, z0, false, u, v, 8, 8);
        quadX(b, z0, z1, y0, y1, x1, true, u + 7, v, 1, 8);
        quadX(b, z0, z1, y0, y1, x0, false, u, v, 1, 8);
        quadY(b, x0, x1, z0, z1, y1, true, u, v, 8, 1);
        quadY(b, x0, x1, z0, z1, y0, false, u, v + 7, 8, 1);
    }

    /*
     * Each quad is given bottom-left, bottom-right, top-right, top-left as seen from outside, which is also the
     * order of the rectangle's corners (0,1) (1,1) (1,0) (0,0): the same convention as the script.
     */

    private static void quadZ(Pinceau b, float x0, float x1, float y0, float y1, float z, boolean front,
                              int u, int v, int w, int h) {
        if (front) {
            quad(b, new Vector3f(x0, y0, z), new Vector3f(x1, y0, z), new Vector3f(x1, y1, z), new Vector3f(x0, y1, z),
                    u, v, w, h);
        } else {
            quad(b, new Vector3f(x1, y0, z), new Vector3f(x0, y0, z), new Vector3f(x0, y1, z), new Vector3f(x1, y1, z),
                    u, v, w, h);
        }
    }

    private static void quadX(Pinceau b, float z0, float z1, float y0, float y1, float x, boolean plus,
                              int u, int v, int w, int h) {
        if (plus) {
            quad(b, new Vector3f(x, y0, z1), new Vector3f(x, y0, z0), new Vector3f(x, y1, z0), new Vector3f(x, y1, z1),
                    u, v, w, h);
        } else {
            quad(b, new Vector3f(x, y0, z0), new Vector3f(x, y0, z1), new Vector3f(x, y1, z1), new Vector3f(x, y1, z0),
                    u, v, w, h);
        }
    }

    private static void quadY(Pinceau b, float x0, float x1, float z0, float z1, float y, boolean top,
                              int u, int v, int w, int h) {
        if (top) {
            quad(b, new Vector3f(x0, y, z1), new Vector3f(x1, y, z1), new Vector3f(x1, y, z0), new Vector3f(x0, y, z0),
                    u, v, w, h);
        } else {
            quad(b, new Vector3f(x0, y, z0), new Vector3f(x1, y, z0), new Vector3f(x1, y, z1), new Vector3f(x0, y, z1),
                    u, v, w, h);
        }
    }

    private static void quad(Pinceau b, Vector3f a, Vector3f c, Vector3f e, Vector3f f, int u, int v, int w, int h) {
        float u0 = (u + MARGIN) / SHEET;
        float v0 = (v + MARGIN) / SHEET;
        float u1 = (u + w - MARGIN) / SHEET;
        float v1 = (v + h - MARGIN) / SHEET;
        // addPoly winds (0, 2, 1), (0, 3, 2): reversed from the order given, hence the corners in reverse.
        b.mesh.addPoly(f, e, c, a);
        Vector3f normal = new Vector3f(c).sub(a).cross(new Vector3f(f).sub(a)).normalize();
        for (int i = 0; i < 4; i++) {
            b.normals.add(normal);
        }
        b.mesh.addTexCoord(u0, v0).addTexCoord(u1, v0).addTexCoord(u1, v1).addTexCoord(u0, v1);
    }

    /** The builder and the normals it does not keep, side by side. */
    private static final class Pinceau {
        private final MeshBuilder mesh = new MeshBuilder();
        private final List<Vector3f> normals = new ArrayList<>();
    }
}
