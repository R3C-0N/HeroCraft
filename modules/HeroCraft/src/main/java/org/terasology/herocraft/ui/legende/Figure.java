// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocraft.ui.legende;

import org.joml.Vector2f;
import org.joml.Vector3f;
import org.terasology.engine.rendering.assets.mesh.Mesh;
import org.terasology.engine.rendering.assets.mesh.MeshBuilder;
import org.terasology.engine.rendering.assets.mesh.StandardMeshData;
import org.terasology.engine.rendering.assets.texture.TextureRegion;
import org.terasology.engine.rendering.iconmesh.IconMeshFactory;
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
 * <p>
 * A weapon in hand is the engine's own held model, the icon raised into one small box per pixel, coloured by its
 * corners rather than by the sheet: its texture coordinates are negative, which the shader reads as « no sheet ».
 */
final class Figure {

    private static final float SHEET = 64f;
    private static final float MARGIN = 0.01f;
    /** Metres per pattern unit for the hand: twelve units make the engine hand's 1.2. */
    private static final float BRAS = 0.1f;
    /** Pattern units per icon unit: an icon spans two, so a sixteen-pixel weapon is fourteen units long. */
    private static final float ARME = 7f;
    /** Where the fist closes on an icon: the arsenal draws every handle from the bottom-left corner. */
    private static final Vector2f POIGNEE = new Vector2f(-0.625f, -0.625f);
    /** The icon's diagonal lowered from 45° to this, so the blade points ahead more than up. */
    private static final float LEVEE = (float) Math.toRadians(-20);
    /** The right fist, in pattern units: the bottom of the arm at x = −6, facing +z. */
    private static final Vector3f POING = new Vector3f(-6, 13, 0.5f);

    private Figure() {
    }

    /**
     * @param largeur how much wider than drawn, across and through: the build
     * @param arme the icon of the weapon held in the right hand, or null for empty hands
     */
    static Mesh build(float largeur, TextureRegion arme) {
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
        int corps = pinceau.normals.size();
        if (arme != null) {
            tenir(pinceau, arme, largeur);
        }

        StandardMeshData data = pinceau.mesh.buildMeshData();
        for (int i = 0; i < corps; i++) {
            data.position.set(i, data.position.get(i, new Vector3f()).mul(largeur, 1, largeur));
        }
        for (int i = 0; i < pinceau.normals.size(); i++) {
            data.normal.put(pinceau.normals.get(i));
            data.color0.put(i < corps ? Color.WHITE : pinceau.couleurs.get(i - corps));
        }
        return Assets.generateAsset(data, Mesh.class);
    }

    /**
     * Puts the icon's model in the right fist, its flat side against the thigh and its blade ahead. The renderer
     * centres the figure on its bounds, and a blade ahead would pull that centre forward and the turn off the
     * body's axis: a point with no area, as far behind as the blade reaches ahead, keeps the bounds square.
     */
    private static void tenir(Pinceau b, TextureRegion arme, float largeur) {
        StandardMeshData icone = (StandardMeshData) IconMeshFactory.generateIconMeshData(arme);
        float cos = (float) Math.cos(LEVEE);
        float sin = (float) Math.sin(LEVEE);
        Vector3f poing = new Vector3f(POING).mul(largeur, 1, largeur);
        float loin = 0;
        int n = icone.position.elements();
        Vector3f[] coin = new Vector3f[4];
        for (int i = 0; i + 3 < n; i += 4) {
            for (int k = 0; k < 4; k++) {
                Vector3f p = icone.position.get(i + k, new Vector3f());
                float x = p.x - POIGNEE.x;
                float y = p.y - POIGNEE.y;
                // the icon's plane turned to face sideways: its x runs ahead, its thickness across
                coin[k] = new Vector3f(-p.z * ARME, (x * sin + y * cos) * ARME, (x * cos - y * sin) * ARME).add(poing);
                loin = Math.max(loin, Math.abs(coin[k].z));
            }
            Vector3f n0 = icone.normal.get(i, new Vector3f());
            Vector3f normal = new Vector3f(-n0.z, n0.x * sin + n0.y * cos, n0.x * cos - n0.y * sin);
            ajouter(b, coin, normal, icone.color0.get(i, new Color()));
        }
        Vector3f arriere = new Vector3f(0, 16, -loin);
        ajouter(b, new Vector3f[] {arriere, arriere, arriere, arriere}, new Vector3f(0, 0, -1), Color.WHITE);
    }

    /** A quad given in the icon's order, (0, 1, 2) and (2, 3, 0), with no place on the sheet. */
    private static void ajouter(Pinceau b, Vector3f[] coin, Vector3f normal, Color couleur) {
        b.mesh.addPoly(coin[3], coin[2], coin[1], coin[0]);
        for (int k = 0; k < 4; k++) {
            b.normals.add(normal);
            b.couleurs.add(couleur);
            b.mesh.addTexCoord(-1, -1);
        }
    }

    /**
     * The right arm alone, for the first-person hand: layer 1 then its sleeve, in metres at a tenth of a pattern
     * unit, turned end over end so the hand is at the top — where the engine's hand box keeps the end nearest the
     * eye — and the box's foot on zero, as that box has it.
     */
    static Mesh bras() {
        Pinceau pinceau = new Pinceau();
        box(pinceau, 0, 0, 4, 12, 4, 40, 16, 0);
        box(pinceau, 0, 0, 4, 12, 4, 40, 32, 0.25f);

        StandardMeshData data = pinceau.mesh.buildMeshData();
        for (int i = 0; i < data.position.elements(); i++) {
            Vector3f p = data.position.get(i, new Vector3f());
            // a half turn about z keeps the winding, which a mirror would reverse
            data.position.set(i, new Vector3f(-p.x, 12 - p.y, p.z).mul(BRAS));
        }
        for (Vector3f normal : pinceau.normals) {
            data.normal.put(new Vector3f(-normal.x, -normal.y, normal.z));
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
        /** Only for what is not on the sheet: the weapon's corners. */
        private final List<Color> couleurs = new ArrayList<>();
    }
}
