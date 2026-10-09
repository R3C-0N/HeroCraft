// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.junit.jupiter.api.Test;
import org.terasology.cubeworlds.projection.ConformalCubeTable;
import org.terasology.cubeworlds.topology.CubeFolding;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.cubeworlds.topology.FacePos;

import java.util.HashSet;
import java.util.Set;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The invariants of the fold. Run at a small face size, because the properties are about the shape
 * of the net and not about its scale — and a small size lets every cell be visited.
 */
class CubeFoldingTest {

    private static final int FACE = 8;

    private static Set<Long> liveCells() {
        Set<Long> live = new HashSet<>();
        for (int x = 0; x < CubePattern.PATTERN_SIDE * FACE; x++) {
            for (int y = 0; y < CubePattern.PATTERN_SIDE * FACE; y++) {
                if (CubePattern.isAlive(x, y, FACE)) {
                    live.add(key(x, y));
                }
            }
        }
        return live;
    }

    private static long key(int x, int y) {
        return ((long) x << 32) | (y & 0xffffffffL);
    }

    @Test
    void theNetHasSixLiveFaces() {
        assertEquals(CubePattern.FACES * FACE * FACE, liveCells().size());
        assertEquals(6, CubePattern.FACES);
        // ten of the sixteen slots are dead: the price of one single definition of the world's shape
        assertEquals(10, CubePattern.PATTERN_SIDE * CubePattern.PATTERN_SIDE - CubePattern.FACES);
    }

    @Test
    void noNeighbourEverLandsOnADeadSlot() {
        int offenders = 0;
        for (long cell : liveCells()) {
            int x = (int) (cell >> 32);
            int y = (int) cell;
            for (int[] n : CubeFolding.neighbours(x, y, FACE)) {
                if (!CubePattern.isAlive(n[0], n[1], FACE)) {
                    offenders++;
                }
            }
        }
        assertEquals(0, offenders, offenders + " neighbours fell on a dead slot");
    }

    @Test
    void exactlyTwentyFourCellsHaveSevenNeighbours() {
        int sevens = 0;
        int anomalies = 0;
        for (long cell : liveCells()) {
            int x = (int) (cell >> 32);
            int y = (int) cell;
            int count = CubeFolding.neighbours(x, y, FACE).length;
            if (count == 7) {
                sevens++;
            } else if (count != 8) {
                anomalies++;
                System.out.printf("cell (%d, %d) has %d neighbours%n", x, y, count);
            }
        }
        System.out.printf("%d cells with seven neighbours, %d anomalies%n", sevens, anomalies);
        assertEquals(0, anomalies, "a cell has neither seven nor eight neighbours");
        assertEquals(24, sevens, "the eight corners of the cube should cost 24 cells a neighbour");
    }

    @Test
    void neighbourhoodIsReciprocal() {
        int faults = 0;
        for (long cell : liveCells()) {
            int x = (int) (cell >> 32);
            int y = (int) cell;
            for (int[] n : CubeFolding.neighbours(x, y, FACE)) {
                boolean back = false;
                for (int[] m : CubeFolding.neighbours(n[0], n[1], FACE)) {
                    if (m[0] == x && m[1] == y) {
                        back = true;
                        break;
                    }
                }
                if (!back) {
                    faults++;
                }
            }
        }
        System.out.printf("non-reciprocal neighbourhoods: %d%n", faults);
        assertEquals(0, faults, "the adjacency graph is not symmetric");
    }

    @Test
    void deadSlotsAreIsolated() {
        int reachable = 0;
        for (int x = 0; x < CubePattern.PATTERN_SIDE * FACE; x++) {
            for (int y = 0; y < CubePattern.PATTERN_SIDE * FACE; y++) {
                if (!CubePattern.isAlive(x, y, FACE) && CubeFolding.neighbours(x, y, FACE).length > 0) {
                    reachable++;
                }
            }
        }
        assertEquals(0, reachable, reachable + " dead cells claim neighbours");
    }

    /**
     * Walking off an edge and walking back must land exactly where it started. This is the check
     * that fails when the fractional part is rotated about the wrong point.
     */
    @Test
    void steppingAcrossASeamAndBackIsExact() {
        double worst = 0.0;
        for (int face = 0; face < CubePattern.FACES; face++) {
            for (int k = 1; k < 16; k++) {
                double along = k * FACE / 16.0;
                for (double[] probe : new double[][]{
                        {FACE + 0.25, along}, {-0.25, along}, {along, FACE + 0.25}, {along, -0.25}}) {
                    FacePos out = CubeFolding.fold(face, probe[0], probe[1], FACE);
                    // step back by the same amount, expressed in the arrival face
                    FacePos back = CubeFolding.fold(out.face(), out.u(), out.v(), FACE);
                    worst = Math.max(worst, Math.hypot(back.u() - out.u(), back.v() - out.v()));
                }
            }
        }
        System.out.printf("re-folding an already folded position moves it by at most %.2e block%n", worst);
        assertTrue(worst < 1e-12, "folding is not idempotent on a position already inside a face");
    }

    /**
     * The seam is only continuous if the world beyond it is the same world. Measured on the sphere,
     * where the question has an answer, rather than in coordinates, where it does not.
     */
    @Test
    void acrossASeamTheSphereDoesNotJump() {
        ConformalCubeTable table = ConformalCubeTable.build();
        int faceBlocks = FACE;
        double worst = 0.0;
        double[] inside = new double[3];
        double[] outside = new double[3];
        for (int face = 0; face < CubePattern.FACES; face++) {
            for (int k = 1; k < FACE; k++) {
                double along = k + 0.5;
                FacePos in = CubeFolding.fold(face, faceBlocks - 0.01, along, faceBlocks);
                FacePos out = CubeFolding.fold(face, faceBlocks + 0.01, along, faceBlocks);
                table.direction(in.face(), 2.0 * in.u() / faceBlocks - 1.0, 2.0 * in.v() / faceBlocks - 1.0, inside);
                table.direction(out.face(), 2.0 * out.u() / faceBlocks - 1.0, 2.0 * out.v() / faceBlocks - 1.0, outside);
                double gap = Math.sqrt(
                        Math.pow(inside[0] - outside[0], 2)
                                + Math.pow(inside[1] - outside[1], 2)
                                + Math.pow(inside[2] - outside[2], 2));
                worst = Math.max(worst, gap);
            }
        }
        // 0.02 of a cell apart on a face of 8 cells: a fifth of a percent of a quarter turn
        System.out.printf("worst gap across a seam: %.2e of a unit sphere%n", worst);
        assertTrue(worst < 0.01, "the sphere jumps across a seam by " + worst);
    }
}
