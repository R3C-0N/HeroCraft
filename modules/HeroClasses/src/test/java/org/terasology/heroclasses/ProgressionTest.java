// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The curve: where each level starts, that the threshold belongs to the new level and not the old one, and that
 * the climb never stops.
 */
public class ProgressionTest {

    @Test
    void everyoneStartsAtLevelOne() {
        assertEquals(0, Progression.experienceFor(1));
        assertEquals(1, Progression.levelFor(0));
        assertEquals(1, Progression.levelFor(-5));
    }

    @Test
    void eachStepCostsOneStepMoreThanTheLast() {
        assertEquals(100, Progression.experienceFor(2));
        assertEquals(300, Progression.experienceFor(3));
        assertEquals(600, Progression.experienceFor(4));
        assertEquals(4500, Progression.experienceFor(10));
    }

    @Test
    void theThresholdBelongsToTheNewLevel() {
        for (int level = 1; level < 2000; level++) {
            long threshold = Progression.experienceFor(level);
            assertEquals(level, Progression.levelFor(threshold), "au seuil du niveau " + level);
            if (threshold > 0) {
                assertEquals(level - 1, Progression.levelFor(threshold - 1), "juste sous le niveau " + level);
            }
        }
    }

    @Test
    void thereIsNoTop() {
        long far = Progression.experienceFor(100_000);
        assertEquals(100_000, Progression.levelFor(far));
        assertTrue(Progression.levelFor(far + Progression.STEP * 100_000L) > 100_000);
    }

    @Test
    void aBeastIsWorthItsHealth() {
        assertEquals(40, Progression.rewardFor(40f));
        assertEquals(320, Progression.rewardFor(320f));
        assertEquals(0, Progression.rewardFor(-1f));
    }
}
