// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.junit.jupiter.api.Test;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.SphereFieldLattice;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.engine.utilities.procedural.BrownianNoise;
import org.terasology.engine.utilities.procedural.Noise;
import org.terasology.engine.utilities.procedural.SimplexNoise;
import org.terasology.engine.world.block.BlockRegion;

import java.util.Arrays;
import java.util.Random;

import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * What the room fields actually reach, against what the swell asks of them.
 * <p>
 * <b>A level set past the range of the noise is a room that exists nowhere</b>, and nothing on
 * screen says so — the world renders, the galleries are there, and the halls simply never open. The
 * ore table learned this the expensive way; the rooms were written with the same mistake still in
 * them, expressed in sigma rather than in quantiles.
 */
class RoomFieldCalibrationTest {

    private static final int LATTICE_STEP = 4;
    private static final int LOWEST = -192;
    private static final int HIGHEST = 120;
    private static final int REGIONS_PER_FACE = 20;
    private static final int STRIDE = 3;

    private static final double[] HORIZONTAL = {70.0, 150.0, 320.0};
    private static final double[] VERTICAL = {34.0, 64.0, 120.0};
    private static final String[] NAMES = {"chamber  70x34", "hall    150x64", "cathedral 320x120"};

    private static final double[] SHARES = {0.10, 0.05, 0.02, 0.01, 0.005, 0.002, 0.001, 0.0005};

    /** The bore each swell buys, on the main gallery: 8.01 wide by 4.55 high at swell one. */
    private static final double BORE_WIDE = 8.01;
    private static final double BORE_HIGH = 4.55;

    @Test
    void theRoomLevelsAreInsideWhatTheFieldReaches() {
        CubeGeometry geometry = CubeGeometry.createDefault();
        float[][] samples = sample(geometry);

        System.out.printf("%d samples per field, lattice step %d%n", samples[0].length, LATTICE_STEP);
        for (int tier = 0; tier < HORIZONTAL.length; tier++) {
            float[] sorted = samples[tier];
            double sigma = sigma(sorted);
            System.out.printf("%s: sigma %.4f, range %.4f .. %.4f%n",
                    NAMES[tier], sigma, sorted[0], sorted[sorted.length - 1]);
            StringBuilder line = new StringBuilder("   quantiles:");
            for (double share : SHARES) {
                line.append(String.format(" %.4f@%.2f%%", quantile(sorted, share), share * 100.0));
            }
            System.out.println(line);

            assertTrue(sorted[sorted.length - 1] > 0.0, NAMES[tier] + " never rose above zero at all");
        }

        System.out.println();
        System.out.println("what a swell buys on the main gallery (8.01 wide x 4.55 high at swell 1):");
        for (double swell : new double[]{2.0, 3.0, 3.5, 5.0, 7.0}) {
            System.out.printf("  swell %.1f -> %.0f wide x %.0f high%n",
                    swell, BORE_WIDE * swell, BORE_HIGH * swell);
        }
    }

    private static float[][] sample(CubeGeometry geometry) {
        int edge = geometry.faceBlocks();
        Random random = new Random(0x8001L);
        int xs = (32 + STRIDE - 1) / STRIDE;
        int ys = (64 + STRIDE - 1) / STRIDE;
        int total = xs * ys * xs * CubePattern.FACES * REGIONS_PER_FACE;

        float[][] collected = new float[HORIZONTAL.length][total];
        Noise[] noises = new Noise[HORIZONTAL.length];
        for (int tier = 0; tier < HORIZONTAL.length; tier++) {
            noises[tier] = new BrownianNoise(new SimplexNoise(0x2c871f45L + tier), 3);
        }
        int[] written = new int[HORIZONTAL.length];

        for (int face = 0; face < CubePattern.FACES; face++) {
            for (int n = 0; n < REGIONS_PER_FACE; n++) {
                int u = align(random.nextInt(edge - 64));
                int v = align(random.nextInt(edge - 64));
                int y = align(LOWEST + random.nextInt(HIGHEST - LOWEST - 64));
                int x = CubePattern.netX(face, u, edge);
                int z = CubePattern.netY(face, v, edge);

                BlockRegion region = new BlockRegion(x, y, z).setSize(32, 64, 32);
                SphereFieldLattice lattice = new SphereFieldLattice(geometry, LATTICE_STEP);
                int[] fields = new int[HORIZONTAL.length];
                for (int tier = 0; tier < HORIZONTAL.length; tier++) {
                    fields[tier] = lattice.addVolumeField(noises[tier], HORIZONTAL[tier], VERTICAL[tier]);
                }
                lattice.prepare(face, region);

                for (int dz = 0; dz < 32; dz += STRIDE) {
                    for (int dy = 0; dy < 64; dy += STRIDE) {
                        for (int dx = 0; dx < 32; dx += STRIDE) {
                            for (int tier = 0; tier < HORIZONTAL.length; tier++) {
                                collected[tier][written[tier]++] =
                                        lattice.volume(fields[tier], x + dx, y + dy, z + dz);
                            }
                        }
                    }
                }
            }
        }
        for (float[] values : collected) {
            Arrays.sort(values);
        }
        return collected;
    }

    private static int align(int value) {
        return Math.floorDiv(value, LATTICE_STEP) * LATTICE_STEP;
    }

    private static double quantile(float[] sorted, double share) {
        int index = (int) Math.floor((1.0 - share) * sorted.length);
        return sorted[Math.max(0, Math.min(sorted.length - 1, index))];
    }

    private static double sigma(float[] values) {
        double sum = 0.0;
        double squares = 0.0;
        for (float value : values) {
            sum += value;
            squares += (double) value * value;
        }
        double mean = sum / values.length;
        return Math.sqrt(squares / values.length - mean * mean);
    }
}
