// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import org.joml.Vector3f;
import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The shapes, read flat: an arc by range and angle, a line by length and width, a circle by radius, the height
 * band for all, and an aim assist that pulls onto a target near the aim and nowhere else.
 */
public class SkillShapesTest {

    private static final Vector3f ORIGIN = new Vector3f();
    private static final Vector3f EAST = new Vector3f(1, 0, 0);

    @Test
    void lookingDownDoesNotShortenTheSwing() {
        Vector3f forward = SkillShapes.flat(new Vector3f(1, -3, 0));
        assertEquals(1, forward.x, 1e-6);
        assertEquals(0, forward.y, 1e-6);
        assertTrue(SkillShapes.inArc(ORIGIN, forward, new Vector3f(2.5f, 0, 0), 3, 100));
    }

    @Test
    void straightUpFallsBackOnSomething() {
        assertEquals(1, SkillShapes.flat(new Vector3f(0, 1, 0)).length(), 1e-6);
    }

    @Test
    void anArcCoversItsAngleAndNoMore() {
        assertTrue(SkillShapes.inArc(ORIGIN, EAST, new Vector3f(1, 0, 1), 3, 100), "45° is inside 100°");
        assertFalse(SkillShapes.inArc(ORIGIN, EAST, new Vector3f(1, 0, 2), 3, 100), "63° is outside");
        assertFalse(SkillShapes.inArc(ORIGIN, EAST, new Vector3f(-1, 0, 0), 3, 100), "behind");
        assertFalse(SkillShapes.inArc(ORIGIN, EAST, new Vector3f(3.5f, 0, 0), 3, 100), "out of range");
    }

    @Test
    void aLineHasALengthAndAWidth() {
        assertTrue(SkillShapes.inLine(ORIGIN, EAST, new Vector3f(5, 0, 0.7f), 6, 1.5f));
        assertFalse(SkillShapes.inLine(ORIGIN, EAST, new Vector3f(5, 0, 0.8f), 6, 1.5f), "too wide");
        assertFalse(SkillShapes.inLine(ORIGIN, EAST, new Vector3f(6.5f, 0, 0), 6, 1.5f), "too far");
        assertFalse(SkillShapes.inLine(ORIGIN, EAST, new Vector3f(-1, 0, 0), 6, 1.5f), "behind");
    }

    @Test
    void aCircleSurroundsTheCaster() {
        assertTrue(SkillShapes.inCircle(ORIGIN, new Vector3f(-2, 0, 2), 3));
        assertFalse(SkillShapes.inCircle(ORIGIN, new Vector3f(-3, 0, 3), 3));
    }

    @Test
    void nothingAboveOrBelowTheBandIsTouched() {
        assertFalse(SkillShapes.inCircle(ORIGIN, new Vector3f(1, 3, 0), 3));
        assertTrue(SkillShapes.inCircle(ORIGIN, new Vector3f(1, 2, 0), 3));
    }

    @Test
    void theAssistPullsOntoATargetNearTheAim() {
        Vector3f target = new Vector3f(4, 0, 0.6f);
        Vector3f aim = SkillShapes.assist(ORIGIN, EAST, List.of(target), 6);
        assertEquals(new Vector3f(target).normalize().z, aim.z, 1e-5);
    }

    @Test
    void theAssistIgnoresWhatIsWideOrFar() {
        Vector3f wide = new Vector3f(2, 0, 2);
        Vector3f far = new Vector3f(9, 0, 0.5f);
        assertEquals(0, SkillShapes.assist(ORIGIN, EAST, List.of(wide, far), 6).z, 1e-6);
    }
}
