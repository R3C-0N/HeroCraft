// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.rendering;

import org.joml.Matrix3f;
import org.joml.Vector3f;
import org.junit.jupiter.api.Test;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.topology.CubePattern;

import java.util.Locale;

import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Whether the focus the CPU hands the shader is the focus the CPU culls against.
 * <p>
 * Two paths compute it. The shader's, transcribed in single precision from the table, is what every
 * vertex subtracts from its own bent position, so that the two cancel under the camera. The CPU's
 * own, in double precision from the exact projection, is what chunks and entities are placed and
 * culled with. They cannot be equal to the bit — one interpolates a table, the other does not — but
 * a drift between them is a gap between where a chunk is tested and where it is drawn.
 * <p>
 * Not at the centre of a face alone: there the table is exact and the two agree to zero, which
 * proves nothing. Off the centre, along an edge, near a corner where the conformal map is raccorded
 * to the equiangular one, and high above the ground.
 */
class FocusAgreementTest {

    /** A hundredth of a block: more than the interpolation of the table can cost at the radius. */
    private static final double POINT_TOLERANCE = 0.01;
    private static final double FRAME_TOLERANCE = 1e-4;

    @Test
    void theShaderAndTheCpuBendTheFocusAlike() {
        CubeGeometry geometry = CubeGeometry.createDefault();
        CubeSphereProjection projection = new CubeSphereProjection(geometry, null);
        int edge = geometry.faceBlocks();
        float sea = geometry.seaLevel();
        // (u, v) inside face 1, as fractions of the face, then a height.
        float[][] places = {
                {0.5f, 0.5f, sea},
                {0.37f, 0.61f, sea},
                {0.5f, 0.02f, sea},
                {0.98f, 0.5f, sea + 40},
                {0.03f, 0.04f, sea},
                {0.97f, 0.985f, sea + 10},
                {0.21f, 0.83f, sea + 200},
        };
        double worstPoint = 0;
        double worstFrame = 0;
        for (float[] place : places) {
            float x = CubePattern.netX(1, 0, edge) + place[0] * edge;
            float z = CubePattern.netY(1, 0, edge) + place[1] * edge;
            projection.setDefaultFocus(x, place[2], z);

            double gap = projection.focusPoint(new Vector3f()).distance(projection.cpuFocusPoint(new Vector3f()));
            Matrix3f shaderFrame = projection.focusFrame(new Matrix3f());
            Matrix3f cpuFrame = projection.cpuFocusFrame(new Matrix3f());
            double frameGap = 0;
            for (int column = 0; column < 3; column++) {
                for (int row = 0; row < 3; row++) {
                    frameGap = Math.max(frameGap, Math.abs(shaderFrame.get(column, row) - cpuFrame.get(column, row)));
                }
            }
            System.out.printf(Locale.ROOT, "u=%.3f v=%.3f y=%.0f: points %.2e blocks apart, frames %.2e%n",
                    place[0], place[1], place[2], gap, frameGap);
            worstPoint = Math.max(worstPoint, gap);
            worstFrame = Math.max(worstFrame, frameGap);

            // Three unit axes. Not a rotation, and it must not be asserted as one: near a corner east
            // and north meet at up to 120 degrees (D27), on the shader's side as on the CPU's.
            for (int row = 0; row < 3; row++) {
                double length = Math.sqrt(shaderFrame.get(0, row) * shaderFrame.get(0, row)
                        + shaderFrame.get(1, row) * shaderFrame.get(1, row)
                        + shaderFrame.get(2, row) * shaderFrame.get(2, row));
                assertTrue(Math.abs(length - 1.0) < 1e-5, "axis " + row + " of the focus frame is not a unit vector");
            }
        }
        assertTrue(worstPoint > 0, "every place agreed to zero: the measure is not looking where the table interpolates");
        assertTrue(worstPoint < POINT_TOLERANCE, "the bent focus drifted by " + worstPoint + " blocks");
        assertTrue(worstFrame < FRAME_TOLERANCE, "the frames at the focus drifted by " + worstFrame);
    }
}
