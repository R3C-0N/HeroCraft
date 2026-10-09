// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.junit.jupiter.api.Test;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.SphereFieldLattice;
import org.terasology.herocubeworlds.generator.SphereSampler;
import org.terasology.herocubeworlds.projection.ConformalCubeTable;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.herocubeworlds.topology.FacePos;
import org.terasology.engine.utilities.procedural.BrownianNoise;
import org.terasology.engine.utilities.procedural.Noise;
import org.terasology.engine.utilities.procedural.SimplexNoise;
import org.terasology.engine.world.block.BlockRegion;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Whether a gallery survives a seam.
 * <p>
 * {@code SeamContinuityTest} already establishes this for the fields that decide the climate, but
 * only in two dimensions, and depth is not a third dimension of the same kind: it does not fold,
 * and {@code readAt} reaches it by a route of its own. A cave field is the first thing in this
 * module read at an altitude, so the property is new even though the mechanism is old.
 * <p>
 * Two measurements, and the second is the one that would catch the fault worth catching. A ratio
 * says the field is of the same order on both sides of a seam; it does not say that a chunk
 * generated as part of one face and its neighbour generated as part of the other put the wall in
 * the same place. Only asking both, about the same point of the sphere, says that.
 */
class CaveSeamContinuityTest {

    private static final int FACE_CHUNKS = 4;
    private static final double HORIZONTAL = 20.0;
    private static final double VERTICAL = 13.0;
    private static final int[] ALTITUDES = {-150, -60, 0, 60};

    private interface Field {
        double at(int face, double u, double v, double y);
    }

    @Test
    void theFieldIsAsContinuousInDepthAsItIsAcrossTheGround() {
        CubeGeometry geometry = new CubeGeometry(FACE_CHUNKS, 32, ConformalCubeTable.build());
        int edge = geometry.faceBlocks();
        Noise noise = new BrownianNoise(new SimplexNoise(4321), 1);

        Field onSphere = (face, u, v, y) -> {
            SphereSampler sampler = new SphereSampler(geometry, face);
            sampler.moveTo(CubePattern.netX(face, 0, edge) + u, CubePattern.netY(face, 0, edge) + v);
            return sampler.readAt(noise, HORIZONTAL, VERTICAL, y);
        };
        // The same noise read where the cell happens to sit in the net, which is what a provider
        // written without the sampler would do. Without something that fails, a number that passes
        // proves nothing.
        Field inGrid = (face, u, v, y) -> {
            FacePos folded = geometry.fold(face,
                    CubePattern.netX(face, 0, edge) + u, CubePattern.netY(face, 0, edge) + v);
            return noise.noise(
                    (float) (folded.netX(edge) / HORIZONTAL),
                    (float) (y / VERTICAL),
                    (float) (folded.netY(edge) / HORIZONTAL));
        };

        int jumpy = 0;
        double worstSphere = 0.0;
        for (int altitude : ALTITUDES) {
            double[] sphere = perEdgeRatios(onSphere, edge, altitude);
            double[] grid = perEdgeRatios(inGrid, edge, altitude);
            for (int i = 0; i < sphere.length; i++) {
                worstSphere = Math.max(worstSphere, sphere[i]);
                if (grid[i] > 2.0) {
                    jumpy++;
                }
                assertTrue(sphere[i] > 0.5 && sphere[i] < 1.5,
                        "at altitude " + altitude + ", seam " + i + " is not continuous: " + sphere[i]);
            }
        }
        System.out.printf("worst seam ratio on the sphere, over four altitudes: %.3f; the flat "
                + "witness jumps on %d of %d edge readings%n", worstSphere, jumpy, 24 * ALTITUDES.length);
        assertEquals(14 * ALTITUDES.length, jumpy, "the witness stopped jumping where it has to");
    }

    /**
     * The measurement with no tolerance in it. Two chunks that meet at a seam are generated from
     * two different faces, so the same point of the sphere is reached by two different folds and
     * two different lattices. If those two answers differ at all, the wall between them is in two
     * places and the gallery is cut in half.
     * <p>
     * The lattice being anchored on the face rather than on the net is what makes this hold, and it
     * is the only reason: a lattice laid out from the net lines up on the ten face edges that are
     * adjacent in the cross and misses on the other fourteen.
     */
    @Test
    void twoFacesPutTheWallInTheSamePlace() {
        // Sixteen chunks and not four: the guard is that a feature must be no larger than a quarter
        // of the radius, and at four chunks the radius is 78.7 against a feature of 20. A field
        // that coarse changes by a large fraction of sigma from one block to the next, and every
        // disagreement here would be swamped by it.
        CubeGeometry geometry = new CubeGeometry(16, 32, ConformalCubeTable.build());
        int edge = geometry.faceBlocks();
        Noise noise = new BrownianNoise(new SimplexNoise(4321), 1);

        int compared = 0;
        double worst = 0.0;
        double worstWithoutLattice = 0.0;
        for (int face = 0; face < CubePattern.FACES; face++) {
            for (int v = 1; v < edge - 1; v += 3) {
                for (int past = 0; past < 2; past++) {
                    // A cell just past this face's +u edge: on the neighbour, whichever that is.
                    //
                    // The integer coordinate is folded, not the centre of the cell. A provider
                    // samples at the integer, so that is the point two faces have to agree about,
                    // and a quarter turn does not send the corner of a cell to the corresponding
                    // corner of its image. Folding the centre and flooring was the first version,
                    // and it compared two points a block apart — the control below caught it.
                    int xFromHere = CubePattern.netX(face, edge + past, edge);
                    int zFromHere = CubePattern.netY(face, v, edge);

                    FacePos there = geometry.fold(face, xFromHere, zFromHere);
                    int xThere = (int) Math.round(there.netX(edge));
                    int zThere = (int) Math.round(there.netY(edge));

                    for (int y : ALTITUDES) {
                        double reachedFromHere = throughLattice(geometry, noise, face, xFromHere, y, zFromHere);
                        double generatedThere = throughLattice(geometry, noise, there.face(), xThere, y, zThere);
                        worst = Math.max(worst, Math.abs(reachedFromHere - generatedThere));
                        // The control that tells a torn lattice apart from a test asking about two
                        // different points: the same two coordinates, straight off the sampler.
                        // If this moves too, the fault is in the mapping and not in the lattice.
                        worstWithoutLattice = Math.max(worstWithoutLattice, Math.abs(
                                direct(geometry, noise, face, xFromHere, y, zFromHere)
                                        - direct(geometry, noise, there.face(), xThere, y, zThere)));
                        compared++;
                    }
                }
            }
        }

        System.out.printf("%d cells beyond a seam, reached from both faces: worst disagreement "
                        + "%.3e through the lattice, %.3e straight off the sampler%n",
                compared, worst, worstWithoutLattice);
        assertTrue(compared > 100, "too few cells were compared for this to mean anything");
        assertTrue(worstWithoutLattice < 1e-6,
                "the two coordinate sets are not the same point of the sphere, so this test is "
                        + "asking the wrong question: the sampler alone already differs by "
                        + worstWithoutLattice);
        assertTrue(worst < 1e-6,
                "the two faces disagree by " + worst + " about the same point of the sphere, so a "
                        + "gallery crossing this seam is cut in two");
    }

    /** The same reading with no lattice in the way. */
    private static double direct(CubeGeometry geometry, Noise noise, int face, int x, int y, int z) {
        SphereSampler sampler = new SphereSampler(geometry, face);
        sampler.moveTo(x, z);
        return sampler.readAt(noise, HORIZONTAL, VERTICAL, y);
    }

    /** One value, through a lattice prepared for the given face exactly as the provider does. */
    private static double throughLattice(CubeGeometry geometry, Noise noise,
                                         int face, int x, int y, int z) {
        SphereFieldLattice lattice = new SphereFieldLattice(geometry, 4);
        int field = lattice.addVolumeField(noise, HORIZONTAL, VERTICAL);
        lattice.prepare(face, new BlockRegion(x, y, z).setSize(1, 1, 1));
        return lattice.volume(field, x, y, z);
    }

    /** @return one ratio per face edge, twenty-four of them, at one altitude */
    private static double[] perEdgeRatios(Field field, int edge, double y) {
        double[] ratios = new double[CubePattern.FACES * 4];
        int margin = Math.max(1, edge / 16);
        for (int face = 0; face < CubePattern.FACES; face++) {
            double[] across = new double[4];
            double[] inside = new double[4];
            for (int w = margin; w < edge - margin; w++) {
                double along = w + 0.5;
                across[0] += Math.abs(field.at(face, edge + 0.5, along, y) - field.at(face, edge - 0.5, along, y));
                inside[0] += Math.abs(field.at(face, edge - 0.5, along, y) - field.at(face, edge - 1.5, along, y));

                across[1] += Math.abs(field.at(face, -0.5, along, y) - field.at(face, 0.5, along, y));
                inside[1] += Math.abs(field.at(face, 0.5, along, y) - field.at(face, 1.5, along, y));

                across[2] += Math.abs(field.at(face, along, edge + 0.5, y) - field.at(face, along, edge - 0.5, y));
                inside[2] += Math.abs(field.at(face, along, edge - 0.5, y) - field.at(face, along, edge - 1.5, y));

                across[3] += Math.abs(field.at(face, along, -0.5, y) - field.at(face, along, 0.5, y));
                inside[3] += Math.abs(field.at(face, along, 0.5, y) - field.at(face, along, 1.5, y));
            }
            for (int e = 0; e < 4; e++) {
                ratios[face * 4 + e] = across[e] / inside[e];
            }
        }
        return ratios;
    }
}
