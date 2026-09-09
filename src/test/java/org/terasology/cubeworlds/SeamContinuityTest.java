// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.junit.jupiter.api.Test;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.SphereSampler;
import org.terasology.cubeworlds.projection.ConformalCubeTable;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.cubeworlds.topology.FacePos;
import org.terasology.engine.utilities.procedural.BrownianNoise;
import org.terasology.engine.utilities.procedural.Noise;
import org.terasology.engine.utilities.procedural.SimplexNoise;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Whether the noise is continuous across the twelve seams.
 * <p>
 * The measurement compares the step the field takes <em>across</em> a seam to the step it takes one
 * cell <em>inside</em> the same face. A continuous field makes the two comparable; a field read in
 * grid coordinates makes the first one enormous.
 * <p>
 * Three things about its shape, each of which took a wrong version to learn. The corners are
 * excluded, because three faces meet there and "the cell across" stops meaning anything. The steps
 * are summed and divided once at the end, never averaged as ratios — an ordinary step passes
 * through zero at every local extremum, and a mean of ratios would be dominated by those. And the
 * whole thing is run a second time against a witness that reads the very same noise in flat grid
 * coordinates, the way the stock providers do: without something that fails, a number that passes
 * proves nothing.
 */
class SeamContinuityTest {

    private static final int FACE_CHUNKS = 4;
    private static final double FEATURE_SIZE = 20.0;

    private interface Field {
        double at(int face, double u, double v);
    }

    @Test
    void theSphereIsContinuousAcrossSeamsAndTheGridIsNot() {
        CubeGeometry geometry = new CubeGeometry(FACE_CHUNKS, 32, ConformalCubeTable.build());
        int edge = geometry.faceBlocks();
        Noise noise = new BrownianNoise(new SimplexNoise(1234), 4);

        // Both fields fold first, so both are asked about the very same cell of the world. They
        // differ only in what they sample once they have it: a direction on the sphere, or the
        // coordinates the cell happens to occupy in the net.
        Field onSphere = (face, u, v) -> {
            SphereSampler sampler = new SphereSampler(geometry, face);
            sampler.moveTo(CubePattern.netX(face, 0, edge) + u, CubePattern.netY(face, 0, edge) + v);
            return sampler.read(noise, FEATURE_SIZE);
        };
        Field inGrid = (face, u, v) -> {
            FacePos folded = geometry.fold(face,
                    CubePattern.netX(face, 0, edge) + u, CubePattern.netY(face, 0, edge) + v);
            return noise.noise(
                    (float) (folded.netX(edge) / FEATURE_SIZE),
                    (float) (folded.netY(edge) / FEATURE_SIZE));
        };

        double[] sphere = perEdgeRatios(onSphere, edge);
        double[] grid = perEdgeRatios(inGrid, edge);

        System.out.println("face edge | on the sphere | read in the grid");
        double worstSphere = 0.0;
        double worstGrid = 0.0;
        int accidental = 0;
        for (int i = 0; i < sphere.length; i++) {
            String label = CubePattern.NAMES[i / 4] + " " + new String[]{"+u", "-u", "+v", "-v"}[i % 4];
            System.out.printf("%-10s|  %8.3f     |  %8.3f%s%n", label, sphere[i], grid[i],
                    grid[i] < 1.5 ? "   <- net-adjacent, flat reading happens to work" : "");
            worstSphere = Math.max(worstSphere, sphere[i]);
            worstGrid = Math.max(worstGrid, grid[i]);
            if (grid[i] < 1.5) {
                accidental++;
            }
        }
        System.out.printf("worst on the sphere %.3f, worst in the grid %.3f, %d of 24 face edges "
                + "are net-adjacent%n", worstSphere, worstGrid, accidental);

        for (int i = 0; i < sphere.length; i++) {
            assertTrue(sphere[i] > 0.5 && sphere[i] < 1.5,
                    "seam " + i + " is not continuous on the sphere: " + sphere[i]);
        }
        // The cross lays five of the twelve cube edges side by side, so ten of the twenty-four
        // face edges are continuous in the net by accident. Asserting the count rather than a
        // magnitude is what makes this falsifiable: the fourteen that remain are exactly the ones
        // a flat reading cannot get right, and if that split ever changes, the net changed.
        int jumpy = 0;
        for (double r : grid) {
            if (r > 2.0) {
                jumpy++;
            }
        }
        assertEquals(14, jumpy, "the witness stopped jumping where it has to");
        assertEquals(10, accidental, "unexpected number of net-adjacent face edges");
    }

    /**
     * @return one ratio per face edge, twenty-four of them, in face-then-edge order
     */
    private static double[] perEdgeRatios(Field field, int edge) {
        double[] ratios = new double[CubePattern.FACES * 4];
        int margin = Math.max(1, edge / 16);
        for (int face = 0; face < CubePattern.FACES; face++) {
            double[] across = new double[4];
            double[] inside = new double[4];
            for (int w = margin; w < edge - margin; w++) {
                double along = w + 0.5;
                across[0] += Math.abs(field.at(face, edge + 0.5, along) - field.at(face, edge - 0.5, along));
                inside[0] += Math.abs(field.at(face, edge - 0.5, along) - field.at(face, edge - 1.5, along));

                across[1] += Math.abs(field.at(face, -0.5, along) - field.at(face, 0.5, along));
                inside[1] += Math.abs(field.at(face, 0.5, along) - field.at(face, 1.5, along));

                across[2] += Math.abs(field.at(face, along, edge + 0.5) - field.at(face, along, edge - 0.5));
                inside[2] += Math.abs(field.at(face, along, edge - 0.5) - field.at(face, along, edge - 1.5));

                across[3] += Math.abs(field.at(face, along, -0.5) - field.at(face, along, 0.5));
                inside[3] += Math.abs(field.at(face, along, 0.5) - field.at(face, along, 1.5));
            }
            for (int e = 0; e < 4; e++) {
                ratios[face * 4 + e] = across[e] / inside[e];
            }
        }
        return ratios;
    }
}
