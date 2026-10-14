// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocraft.ui.legende;

import org.joml.Matrix3f;
import org.joml.Matrix4f;
import org.joml.Vector3f;

/**
 * How a hero holds what is in its right hand: one rule for the creator's figure, for the body in the world, and for
 * the arm seen in first person, so the three cannot drift apart.
 * <p>
 * Everything here is in pattern units, the hero's own measure (32 from feet to crown, facing +z, the right arm at
 * x = −6): each place that draws the hero puts its own pattern-to-screen transform in front.
 * <p>
 * A held item's model is the engine's: an icon raised into one small box per pixel, spanning [−1, 1] across and up,
 * or a block one unit wide, centred.
 */
final class Prise {

    /** Pattern units per icon unit: an icon spans two, so a sixteen-pixel weapon is fourteen units long. */
    private static final float ARME = 7f;
    /** A held block fills the fist: four units, the arm's width. */
    private static final float BLOC = 4f;
    /** Where the fist closes on an icon: the arsenal draws every handle from the bottom-left corner. */
    private static final Vector3f POIGNEE = new Vector3f(-0.625f, -0.625f, 0);
    /** The icon's diagonal lowered from 45° to this, so the blade points ahead more than up. */
    private static final float LEVEE = (float) Math.toRadians(-20);
    /** The right fist: the bottom of the arm at x = −6, a little forward. */
    static final Vector3f POING = new Vector3f(-6, 13, 0.5f);
    /** The top of the right arm, where its bone turns it. */
    static final Vector3f EPAULE = new Vector3f(-6, 24, 0);

    private Prise() {
    }

    /**
     * From the item's model to the pattern, the fist given: the icon's plane turned to face sideways, its x running
     * ahead and its thickness across, its handle in the fist; a block simply centred in it.
     */
    static Matrix4f objet(boolean bloc, Vector3f poing, Matrix4f dest) {
        if (bloc) {
            return dest.translation(poing).scale(BLOC);
        }
        float cos = (float) Math.cos(LEVEE);
        float sin = (float) Math.sin(LEVEE);
        // columns: where the icon's x, y and z go
        Matrix3f tour = new Matrix3f(
                0, sin, cos,
                0, cos, -sin,
                -1, 0, 0);
        return dest.translation(poing).mul(new Matrix4f().set(tour)).scale(ARME).translate(-POIGNEE.x, -POIGNEE.y, 0);
    }

    static Matrix4f objet(boolean bloc, Matrix4f dest) {
        return objet(bloc, POING, dest);
    }

    /** Where the fist closes, in the item's own model: the handle of an icon, the bottom face of a block. */
    static Vector3f poignee(boolean bloc) {
        return bloc ? new Vector3f(0, -0.5f, 0) : new Vector3f(POIGNEE);
    }
}
