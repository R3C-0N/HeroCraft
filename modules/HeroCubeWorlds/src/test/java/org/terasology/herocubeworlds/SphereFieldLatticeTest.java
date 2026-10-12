// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.junit.jupiter.api.Test;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.SphereFieldLattice;
import org.terasology.herocubeworlds.generator.SphereSampler;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.engine.utilities.procedural.BrownianNoise;
import org.terasology.engine.utilities.procedural.Noise;
import org.terasology.engine.utilities.procedural.SimplexNoise;
import org.terasology.engine.world.block.BlockRegion;

import java.util.Random;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The lattice, measured before anything is built on it.
 * <p>
 * Three separate things can be wrong with a sub-sampler and only one of them is visible in the
 * others' tests: it can be too coarse for the field, it can disagree with itself between two
 * regions, and it can quietly not be doing any sub-sampling at all. One test each.
 * <p>
 * On the production geometry throughout. A cave bore is written in blocks because a player is 1.6
 * blocks tall, so it does not scale down with the world: on a small test world the same numbers
 * carve almost everything, and every figure here would be measured on a field that will never be
 * generated.
 */
class SphereFieldLatticeTest {

    private static final int STEP = 4;
    private static final double TUNNEL_H = 64.0;
    private static final double TUNNEL_V = 42.0;

    private static CubeGeometry production() {
        return CubeGeometry.createDefault();
    }

    /** A chunk-sized region of solid rock at the middle of face 1. */
    private static BlockRegion aChunkOfRock(CubeGeometry geometry) {
        int edge = geometry.faceBlocks();
        int x = CubePattern.netX(1, edge / 2, edge);
        int z = CubePattern.netY(1, edge / 2, edge);
        return new BlockRegion(x, -64, z).setSize(32, 64, 32);
    }

    @Test
    void theLatticeFollowsTheFieldItSamples() {
        CubeGeometry geometry = production();
        BlockRegion region = aChunkOfRock(geometry);
        Noise noise = new BrownianNoise(new SimplexNoise(0xCAFEL), 1);

        SphereFieldLattice lattice = new SphereFieldLattice(geometry, STEP);
        int field = lattice.addVolumeField(noise, TUNNEL_H, TUNNEL_V);
        lattice.prepare(1, region);

        SphereSampler sampler = new SphereSampler(geometry, 1);
        Random random = new Random(7);
        int samples = 20000;
        double sumSquares = 0.0;
        double sumExactSquares = 0.0;
        double sumExact = 0.0;
        double worst = 0.0;
        for (int i = 0; i < samples; i++) {
            int x = region.minX() + random.nextInt(region.getSizeX());
            int y = region.minY() + random.nextInt(region.getSizeY());
            int z = region.minZ() + random.nextInt(region.getSizeZ());
            sampler.moveTo(x, z);
            double exact = sampler.readAt(noise, TUNNEL_H, TUNNEL_V, y);
            double error = lattice.volume(field, x, y, z) - exact;
            sumSquares += error * error;
            sumExact += exact;
            sumExactSquares += exact * exact;
            worst = Math.max(worst, Math.abs(error));
        }
        double mean = sumExact / samples;
        double sigma = Math.sqrt(sumExactSquares / samples - mean * mean);
        double rms = Math.sqrt(sumSquares / samples);

        System.out.printf("field sigma %.4f; lattice step %d: rms error %.5f (%.2f %% of sigma), "
                        + "worst %.5f (%.2f %% of sigma)%n",
                sigma, STEP, rms, 100 * rms / sigma, worst, 100 * worst / sigma);

        assertTrue(sigma > 0.05, "the field is flat here, so this measures nothing: sigma " + sigma);
        assertTrue(rms < 0.08 * sigma, "the lattice is too coarse for this field: rms is "
                + (100 * rms / sigma) + " % of sigma");
        assertTrue(worst < 0.30 * sigma, "the lattice misses the field badly somewhere: worst is "
                + (100 * worst / sigma) + " % of sigma");
    }

    /**
     * The property that keeps a wall from tearing between two chunks: the lattice is one global
     * lattice, not a grid laid out from whatever region happens to be asked for. The second region
     * is offset by seventeen blocks on purpose — a multiple of the step would line the two origins
     * up by accident and the test would pass on an implementation anchored to the region.
     */
    @Test
    void twoRegionsAgreeExactlyWhereTheyOverlap() {
        CubeGeometry geometry = production();
        BlockRegion first = aChunkOfRock(geometry);
        BlockRegion second = new BlockRegion(first.minX() + 17, first.minY() + 17, first.minZ() + 17)
                .setSize(32, 64, 32);
        Noise noise = new BrownianNoise(new SimplexNoise(0xCAFEL), 1);

        SphereFieldLattice a = new SphereFieldLattice(geometry, STEP);
        int fieldA = a.addVolumeField(noise, TUNNEL_H, TUNNEL_V);
        a.prepare(1, first);

        SphereFieldLattice b = new SphereFieldLattice(geometry, STEP);
        int fieldB = b.addVolumeField(noise, TUNNEL_H, TUNNEL_V);
        b.prepare(1, second);

        int compared = 0;
        for (int x = second.minX(); x <= first.maxX(); x++) {
            for (int y = second.minY(); y <= first.maxY(); y += 3) {
                for (int z = second.minZ(); z <= first.maxZ(); z += 3) {
                    assertEquals(a.volume(fieldA, x, y, z), b.volume(fieldB, x, y, z), 0.0f,
                            "the two lattices disagree at " + x + "," + y + "," + z);
                    compared++;
                }
            }
        }
        System.out.printf("%d positions agree bit for bit across two overlapping regions%n", compared);
        assertTrue(compared > 1000, "the regions barely overlap, this proves little");
    }

    @Test
    void theSubSamplingIsActuallyInForce() {
        CubeGeometry geometry = production();
        BlockRegion region = aChunkOfRock(geometry);
        AtomicInteger calls = new AtomicInteger();

        SphereFieldLattice lattice = new SphereFieldLattice(geometry, STEP);
        // The production shape: four tunnel fields at one octave, a room field at three, and one
        // column field at two.
        for (int i = 0; i < 4; i++) {
            lattice.addVolumeField(new BrownianNoise(counting(calls, 0xCAFEL + i), 1), TUNNEL_H, TUNNEL_V);
        }
        lattice.addVolumeField(new BrownianNoise(counting(calls, 0xBEEFL), 3), 110.0, 50.0);
        lattice.addColumnField(new BrownianNoise(counting(calls, 0xF00DL), 2), 120.0);
        lattice.prepare(1, region);

        int voxels = region.getSizeX() * region.getSizeY() * region.getSizeZ();
        System.out.printf("%d simplex evaluations for a chunk of %d cells, %.0fx fewer than one "
                        + "call per cell per field%n",
                calls.get(), voxels, (voxels * 7.0 + voxels * 2.0) / calls.get());
        assertTrue(calls.get() <= 12000,
                "the sub-sampling is not in force: " + calls.get() + " evaluations for one chunk");
        assertTrue(calls.get() > 1000, "nothing was evaluated at all, so nothing is being measured");
    }

    @Test
    void aStepThatDoesNotDivideAFaceIsRefused() {
        CubeGeometry geometry = production();
        // 3200 is not a multiple of 3. A lattice on such a step would line up across the ten seams
        // that are adjacent in the net and break on the other fourteen, in silence.
        assertThrows(IllegalArgumentException.class, () -> new SphereFieldLattice(geometry, 3));
    }

    private static Noise counting(AtomicInteger calls, long seed) {
        SimplexNoise inner = new SimplexNoise(seed);
        return new Noise() {
            @Override
            public float noise(float x, float y) {
                calls.incrementAndGet();
                return inner.noise(x, y);
            }

            @Override
            public float noise(float x, float y, float z) {
                calls.incrementAndGet();
                return inner.noise(x, y, z);
            }

            @Override
            public float noise(int x, int y) {
                return noise((float) x, (float) y);
            }

            @Override
            public float noise(int x, int y, int z) {
                return noise((float) x, (float) y, (float) z);
            }
        };
    }
}
