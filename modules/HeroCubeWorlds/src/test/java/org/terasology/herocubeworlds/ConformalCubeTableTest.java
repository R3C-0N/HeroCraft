// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.projection.ConformalCubeTable;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.engine.world.chunks.Chunks;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The measurements that decide whether the projection is usable. Thresholds are the ones the
 * Veloren bench settled on, so a regression here is comparable to a known good state.
 */
class ConformalCubeTableTest {

    private static ConformalCubeTable table;

    @BeforeAll
    static void buildTable() {
        long start = System.nanoTime();
        table = ConformalCubeTable.build();
        System.out.printf("table %dx%d built in %.0f ms%n",
                table.resolution(), table.resolution(), (System.nanoTime() - start) / 1e6);
    }

    @Test
    void theEquationReachesTheCornerTheoryPredicts() {
        double[] reached = ConformalCubeTable.integrateCorner(4096);
        double[] expected = ConformalCubeTable.expectedCorner();
        double error = Math.hypot(reached[0] - expected[0], reached[1] - expected[1]);
        System.out.printf("corner reached (%.9f, %.9f), expected (%.9f, %.9f), error %.2e%n",
                reached[0], reached[1], expected[0], expected[1], error);
        assertTrue(error < 1e-5, "corner off by " + error);

        double[] gnomonic = ConformalCubeTable.gnomonicOf(reached[0], reached[1]);
        System.out.printf("corner in gnomonic coordinates: (%.6f, %.6f), expected (1, 1)%n",
                gnomonic[0], gnomonic[1]);
        assertEquals(1.0, gnomonic[0], 1e-4);
        assertEquals(1.0, gnomonic[1], 1e-4);
    }

    @Test
    void everyDirectionIsUnit() {
        double worst = 0.0;
        double[] dir = new double[3];
        for (int face = 0; face < CubePattern.FACES; face++) {
            for (int i = 0; i <= 64; i++) {
                for (int j = 0; j <= 64; j++) {
                    double s = -1.0 + 2.0 * i / 64.0;
                    double t = -1.0 + 2.0 * j / 64.0;
                    table.direction(face, s, t, dir);
                    double len = Math.sqrt(dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]);
                    worst = Math.max(worst, Math.abs(len - 1.0));
                }
            }
        }
        System.out.printf("worst deviation from unit length: %.2e%n", worst);
        assertTrue(worst < 1e-9, "directions are not unit: " + worst);
    }

    @Test
    void awayFromTheCornersTheCellStaysSquare() {
        double worstAngle = 0.0;
        double step = 2.0 / 512.0;
        double limit = 1.0 - ConformalCubeTable.BLEND_RADIUS * 1.5;
        for (int i = -20; i <= 20; i++) {
            for (int j = -20; j <= 20; j++) {
                double s = i / 20.0 * limit;
                double t = j / 20.0 * limit;
                double[] d0 = table.localDirection(s, t);
                double[] ds = table.localDirection(s + step, t);
                double[] dt = table.localDirection(s, t + step);
                double[] tu = {ds[0] - d0[0], ds[1] - d0[1], ds[2] - d0[2]};
                double[] tv = {dt[0] - d0[0], dt[1] - d0[1], dt[2] - d0[2]};
                double dot = tu[0] * tv[0] + tu[1] * tv[1] + tu[2] * tv[2];
                double lu = Math.sqrt(tu[0] * tu[0] + tu[1] * tu[1] + tu[2] * tu[2]);
                double lv = Math.sqrt(tv[0] * tv[0] + tv[1] * tv[1] + tv[2] * tv[2]);
                double angle = Math.toDegrees(Math.acos(Math.max(-1, Math.min(1, dot / (lu * lv)))));
                worstAngle = Math.max(worstAngle, Math.abs(angle - 90.0));
            }
        }
        System.out.printf("outside the blend, worst departure from a right angle: %.4f deg%n", worstAngle);
        assertTrue(worstAngle < 1.0, "cells are not square: " + worstAngle + " deg off");
    }

    @Test
    void theProjectionIsInvertible() {
        double worst = 0.0;
        double[] dir = new double[3];
        double[] back = new double[3];
        for (int face = 0; face < CubePattern.FACES; face++) {
            for (int i = 1; i < 32; i++) {
                for (int j = 1; j < 32; j++) {
                    double s = -1.0 + 2.0 * i / 32.0;
                    double t = -1.0 + 2.0 * j / 32.0;
                    table.direction(face, s, t, dir);
                    table.fromDirection(dir[0], dir[1], dir[2], back);
                    assertEquals(face, (int) back[0], "landed on the wrong face");
                    worst = Math.max(worst, Math.hypot(back[1] - s, back[2] - t));
                }
            }
        }
        // s spans 2 across a face, so one unit of s is half a face edge. Derived and not written
        // down: with 4096 hard-coded, shrinking the world left the printed figure 2.56x too large
        // and the assertion in the wrong units, while still passing.
        double faceEdge = CubeGeometry.DEFAULT_FACE_CHUNKS * (double) Chunks.SIZE_X;
        double blocksPerUnit = faceEdge / 2.0;
        System.out.printf("worst round trip error: %.2e in (s,t), i.e. %.4f block at edge %.0f%n",
                worst, worst * blocksPerUnit, faceEdge);
        assertTrue(worst * blocksPerUnit < 1e-3,
                "round trip drifts by " + worst * blocksPerUnit + " block");
    }
}
