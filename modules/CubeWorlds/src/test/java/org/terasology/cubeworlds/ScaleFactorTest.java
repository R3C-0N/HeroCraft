// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import org.terasology.cubeworlds.projection.ConformalCubeTable;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * How big a block is, and in which direction.
 * <p>
 * Measured per direction, never averaged: the blend is the one place where the two disagree, and an
 * average is exactly the shape of measurement that would hide it.
 */
class ScaleFactorTest {

    private static ConformalCubeTable table;
    private static double step;

    @BeforeAll
    static void buildTable() {
        table = ConformalCubeTable.build();
        step = table.step();
    }

    private static double alongS(double s, double t) {
        return table.directionalScale(s, t, step, true);
    }

    private static double alongT(double s, double t) {
        return table.directionalScale(s, t, step, false);
    }

    @Test
    void theSmallestBlockSitsOnAnEdgeNotOnADiagonal() {
        int samples = 512;
        double smallest = Double.MAX_VALUE;
        double atS = 0;
        double atT = 0;
        for (int i = 0; i <= samples; i++) {
            for (int j = 0; j <= samples; j++) {
                double s = -1.0 + 2.0 * i / samples;
                double t = -1.0 + 2.0 * j / samples;
                double lo = Math.min(alongS(s, t), alongT(s, t));
                if (lo < smallest) {
                    smallest = lo;
                    atS = s;
                    atT = t;
                }
            }
        }
        System.out.printf("smallest block %.4f at (s=%.3f, t=%.3f)%n", smallest, atS, atT);

        double onEdge = Math.max(Math.abs(atS), Math.abs(atT));
        double offDiagonal = Math.abs(Math.abs(atS) - Math.abs(atT));
        assertEquals(1.0, onEdge, 1e-9, "the smallest block should sit on an edge");
        assertTrue(offDiagonal > 0.05, "the smallest block should not sit on a diagonal");
        assertTrue(smallest > 0.25 && smallest < 0.26, "smallest block moved to " + smallest);
    }

    @Test
    void theMapIsIsotropicWhereverItIsConformal() {
        double limit = 1.0 - ConformalCubeTable.BLEND_RADIUS * 1.5;
        double worst = 1.0;
        for (int i = -32; i <= 32; i++) {
            for (int j = -32; j <= 32; j++) {
                double s = i / 32.0 * limit;
                double t = j / 32.0 * limit;
                double ls = alongS(s, t);
                double lt = alongT(s, t);
                worst = Math.max(worst, Math.max(ls, lt) / Math.min(ls, lt));
            }
        }
        System.out.printf("outside the blend, worst anisotropy %.5f%n", worst);
        assertTrue(worst < 1.01, "the conformal region is not isotropic: " + worst);
    }

    @Test
    void insideTheBlendTheTwoDirectionsPartCompany() {
        double worst = 1.0;
        double atS = 0;
        double atT = 0;
        for (int i = 0; i <= 512; i++) {
            for (int j = 0; j <= 512; j++) {
                double s = -1.0 + 2.0 * i / 512.0;
                double t = -1.0 + 2.0 * j / 512.0;
                double ls = alongS(s, t);
                double lt = alongT(s, t);
                double ratio = Math.max(ls, lt) / Math.min(ls, lt);
                if (ratio > worst) {
                    worst = ratio;
                    atS = s;
                    atT = t;
                }
            }
        }
        System.out.printf("worst anisotropy %.3f at (s=%.3f, t=%.3f)%n", worst, atS, atT);
        // Recorded, not wished away: this is what fixes the shape of the stored factor, and it is
        // the reason blocks turn to rhombi in the eight blend patches.
        assertTrue(worst > 2.0, "anisotropy vanished, the blend is no longer where it was");
        assertTrue(worst < 3.0, "anisotropy grew to " + worst);
    }

    @Test
    void theStoredFactorIsTheGeometricMean() {
        double worst = 0.0;
        for (int i = 0; i <= 128; i++) {
            for (int j = 0; j <= 128; j++) {
                double s = -1.0 + 2.0 * i / 128.0;
                double t = -1.0 + 2.0 * j / 128.0;
                double expected = Math.sqrt(alongS(s, t) * alongT(s, t));
                worst = Math.max(worst, Math.abs(table.lambda(s, t) - expected));
            }
        }
        System.out.printf("stored factor departs from the geometric mean by at most %.2e%n", worst);
        assertTrue(worst < 2e-3, "stored factor is not the geometric mean: " + worst);
        assertEquals(1.0, table.lambda(0.0, 0.0), 1e-3, "the face centre should be exactly to scale");
    }
}
