// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.rendering;

import org.joml.Vector3f;
import org.junit.jupiter.api.Test;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.herocubeworlds.topology.FacePos;

import java.util.Locale;

import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Whether the two sides of a cut seam are drawn where they meet.
 * <p>
 * A block is centred on its coordinates, so the outer corners of the blocks along an edge of a face lie half a block
 * past it. Along a cut seam that half block is a dead slot, which belongs to no face: its vertices used to be sent to a
 * fixed direction on the far side of the planet, and the edge of each face was drawn as a sheet of triangles stretched
 * across the sky, with a gap between the two faces. They are now brought back onto the edge of their own face, so the
 * vertex of one side and the matching vertex of the other side land on the same point of the sphere.
 * <p>
 * Measured at the edge between face 0 and face 5, cut in the net and glued on the sphere, from its middle to the
 * corner where it meets face 1.
 */
class CutSeamMeetsTest {

    @Test
    void bothSidesOfACutSeamLandOnTheSamePoint() {
        CubeGeometry geometry = CubeGeometry.createDefault();
        CubeSphereProjection projection = new CubeSphereProjection(geometry, null);
        int edge = geometry.faceBlocks();
        float sea = geometry.seaLevel();
        // Face 0 is slot (0, 1): its first row, z = edge, lies against the dead slot (0, 0).
        projection.setDefaultFocus(edge - 10, sea, edge + 10);
        double worst = 0;
        double farthest = 0;
        for (int x = 100; x < edge; x += 250) {
            // The outer corner of a block of face 0 on the seam, half a block into the dead slot.
            Vector3f here = projection.curvedPosition(new Vector3f(x - 0.5f, sea, edge - 0.5f), new Vector3f());
            // The same point of the world, reached from the other face through the folding.
            FacePos across = geometry.fold(0, x, edge - 1e-6);
            float acrossX = CubePattern.netX(across.face(), 0, edge) + (float) across.u();
            float acrossZ = CubePattern.netY(across.face(), 0, edge) + (float) across.v();
            // Its own outer corner, half a block into the same dead slot from the other side.
            float cornerX = acrossX - 0.5f;
            float cornerZ = acrossZ - 0.5f;
            Vector3f there = projection.curvedPosition(new Vector3f(cornerX, sea, cornerZ), new Vector3f());
            double gap = here.distance(there);
            // And each corner stays by its own block: sent anywhere else, both would still agree with each other.
            Vector3f block = projection.curvedPosition(new Vector3f(x, sea, edge), new Vector3f());
            double off = here.distance(block);
            System.out.printf(Locale.ROOT, "x=%4d -> face %d (%.1f, %.1f): %.3f blocks apart, %.3f from its block%n",
                    x, across.face(), acrossX, acrossZ, gap, off);
            worst = Math.max(worst, gap);
            farthest = Math.max(farthest, off);
        }
        // Half a block of each side collapses onto the edge: the two corners are at most that far apart.
        assertTrue(worst < 1.0, "the two sides of the cut seam are " + worst + " blocks apart");
        // A corner is within a block's diagonal of the centre of its block, at the largest a block gets.
        assertTrue(farthest < 1.5, "a corner of a seam block was drawn " + farthest + " blocks from it");
    }
}
