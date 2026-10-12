// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses;

/**
 * The rules of the climb, without an entity in sight, so they can be read and tested on their own.
 * <p>
 * <strong>There is no top.</strong> Each level asks {@link #STEP} more experience than the one before it, so
 * reaching level {@code n} costs {@code STEP · n(n−1)/2} in all: the curve slows the climb without ever
 * closing it, and a long enough hunt always pays.
 * <p>
 * <strong>A beast is worth its health.</strong> The reward is read from the creature's maximum health rather
 * than from a table per species: nineteen numbers to keep in step with nineteen prefabs would drift, while
 * health already says how long the fight lasted. A wolf (40) brings 40, the champion (320) brings 320, and
 * level 2 falls after two or three wolves.
 */
public final class Progression {
    /** What the step from level 1 to level 2 costs, and what each next step adds. */
    public static final int STEP = 100;

    /** Skill points a new level brings. */
    public static final int POINTS_PER_LEVEL = 1;

    private Progression() {
    }

    /** Total experience at which {@code level} is reached; level 1 is where everyone starts. */
    public static long experienceFor(int level) {
        long n = Math.max(1, level);
        return STEP * n * (n - 1) / 2;
    }

    /** The level a total of experience stands at. */
    public static int levelFor(long experience) {
        if (experience <= 0) {
            return 1;
        }
        // n(n−1)/2 ≤ xp/STEP, solved for n and then corrected for rounding on either side
        int level = (int) Math.floor((1 + Math.sqrt(1 + 8.0 * experience / STEP)) / 2);
        while (experienceFor(level + 1) <= experience) {
            level++;
        }
        while (level > 1 && experienceFor(level) > experience) {
            level--;
        }
        return level;
    }

    /** What killing a creature of this maximum health is worth. */
    public static int rewardFor(float maxHealth) {
        return Math.max(0, Math.round(maxHealth));
    }
}
