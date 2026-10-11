// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.junit.jupiter.api.Test;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.OreKind;
import org.terasology.herocubeworlds.generator.SphereFieldLattice;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.engine.utilities.procedural.BrownianNoise;
import org.terasology.engine.utilities.procedural.Noise;
import org.terasology.engine.utilities.procedural.SimplexNoise;
import org.terasology.engine.world.block.BlockRegion;

import java.util.Arrays;
import java.util.Random;

import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * What the ore thresholds are read off, and it is the distribution rather than a formula.
 * <p>
 * <b>A Gaussian tail is the wrong instrument here.</b> The engine's simplex is bounded to [-1, 1],
 * so past a certain rarity the threshold a tail asks for is a number the field never reaches, and a
 * generator set to it places nothing at all. That failure is invisible: the world looks normal and
 * the ore is simply absent. So the thresholds are quantiles of the measured field, and this test
 * also prints the largest value each field ever reached — the number that says whether a target is
 * attainable at all.
 * <p>
 * <b>And it measures the field the provider reads, not the noise underneath it.</b>
 * {@code CubeOreProvider} reads through {@link SphereFieldLattice}, which evaluates the noise on a
 * four-block lattice and interpolates between the nodes. Interpolation shaves the peaks, so the
 * same threshold takes a smaller share of an interpolated field than of the raw one. Calibrating
 * against the raw noise would leave every ore quietly rarer than the table says, by a factor nobody
 * would think to look for.
 * <p>
 * The numbers this prints are the ones quoted in {@link OreKind}. The assertions are wide, because
 * their job is to fail loudly when the ground moves, not to pin a digit.
 */
class OreFieldCalibrationTest {

    /** The band of the world that has rock in it. */
    private static final int LOWEST = -192;
    private static final int HIGHEST = 120;

    /** The lattice the ore provider uses. Calibrating on a different one measures another field. */
    private static final int LATTICE_STEP = 4;

    /** Sample regions per face, and the stride within one. */
    private static final int REGIONS_PER_FACE = 25;

    /**
     * Three, and deliberately coprime with the lattice step.
     * <p>
     * A stride of two would only ever land on lattice nodes and their mid-edges, where the
     * interpolation does the least; the quantiles would come out closer to the raw noise than what
     * the generator sees. Three walks all four phases of the step in turn.
     */
    private static final int STRIDE = 3;

    /** Shares worth printing a threshold for, so retuning a share does not need another run. */
    private static final double[] SHARES = {0.0500, 0.0100, 0.0060, 0.0049, 0.0040, 0.0030, 0.0025, 0.0019};

    @Test
    void theOreThresholdsAreTheQuantilesTheyClaimToBe() {
        CubeGeometry geometry = CubeGeometry.createDefault();
        OreKind[] owners = OreKind.values();

        float[][] samples = sample(geometry, owners);

        System.out.printf("%d fields, %d samples each, lattice step %d%n",
                owners.length, samples[0].length, LATTICE_STEP);
        for (int i = 0; i < owners.length; i++) {
            report(owners[i], samples[i]);
        }

        System.out.println();
        System.out.println("what each ore asks for, against what its field can give:");
        for (OreKind ore : OreKind.values()) {
            float[] sorted = samples[ore.ordinal()];
            double wanted = quantile(sorted, ore.share());
            double largest = sorted[sorted.length - 1];
            System.out.printf("  %-11s share %.4f -> threshold %.4f (in table: %.4f), peak %.4f%s%n",
                    ore, ore.share(), wanted, ore.threshold(), largest,
                    wanted >= largest ? "   UNREACHABLE" : "");

            assertTrue(wanted < largest,
                    ore + " asks for a threshold of " + wanted + " on a field that never exceeds "
                            + largest + ", so it would be generated nowhere at all. Rarity past what "
                            + "the field can carry is bought with a district, never with a threshold.");
        }

        System.out.println();
        System.out.println("thresholds by share, for retuning:");
        for (int i = 0; i < owners.length; i++) {
            StringBuilder line = new StringBuilder(String.format("  %-11s", owners[i]));
            for (double share : SHARES) {
                line.append(String.format(" %.4f@%.2f%%", quantile(samples[i], share), share * 100.0));
            }
            System.out.println(line);
        }

        System.out.println();
        System.out.println("what the table buys, if the thresholds above are adopted:");
        for (OreKind ore : OreKind.values()) {
            double box = ore.horizontalSize() * ore.horizontalSize() * ore.verticalSize();
            System.out.printf("  %-11s vein %.1f blocks, one field box every %.0f cells%n",
                    ore, ore.share() * box, box);
        }
    }

    /** @return one sorted array of field values per owner, in the order the owners were given */
    private static float[][] sample(CubeGeometry geometry, OreKind[] owners) {
        int edge = geometry.faceBlocks();
        Random random = new Random(0x0BE1);
        float[][] collected = new float[owners.length][];
        int[] written = new int[owners.length];

        int perRegion = countPerRegion();
        int total = perRegion * CubePattern.FACES * REGIONS_PER_FACE;
        Noise[] noises = new Noise[owners.length];
        for (int i = 0; i < owners.length; i++) {
            collected[i] = new float[total];
            // Three octaves, which is what narrows sigma enough for the thresholds the table wants
            // to sit inside the range the noise actually reaches. One field per owner, built once:
            // rebuilding it per region would work, but a field is a thing, not a call.
            noises[i] = new BrownianNoise(new SimplexNoise(0x51E0L + i), 3);
        }

        for (int face = 0; face < CubePattern.FACES; face++) {
            for (int n = 0; n < REGIONS_PER_FACE; n++) {
                // Anchored to the lattice step for the same reason the lattice is: a node on a seam
                // plane must sit at the same face-local place seen from either side.
                int u = align(random.nextInt(edge - 64));
                int v = align(random.nextInt(edge - 64));
                int y = align(LOWEST + random.nextInt(HIGHEST - LOWEST - 64));
                int x = CubePattern.netX(face, u, edge);
                int z = CubePattern.netY(face, v, edge);

                BlockRegion region = new BlockRegion(x, y, z).setSize(32, 64, 32);
                SphereFieldLattice lattice = new SphereFieldLattice(geometry, LATTICE_STEP);
                int[] fields = new int[owners.length];
                for (int i = 0; i < owners.length; i++) {
                    fields[i] = lattice.addVolumeField(noises[i], owners[i].horizontalSize(), owners[i].verticalSize());
                }
                lattice.prepare(face, region);

                for (int dz = 0; dz < 32; dz += STRIDE) {
                    for (int dy = 0; dy < 64; dy += STRIDE) {
                        for (int dx = 0; dx < 32; dx += STRIDE) {
                            for (int i = 0; i < owners.length; i++) {
                                collected[i][written[i]++] = lattice.volume(fields[i], x + dx, y + dy, z + dz);
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

    private static int countPerRegion() {
        int xs = (32 + STRIDE - 1) / STRIDE;
        int ys = (64 + STRIDE - 1) / STRIDE;
        int zs = (32 + STRIDE - 1) / STRIDE;
        return xs * ys * zs;
    }

    private static int align(int value) {
        return Math.floorDiv(value, LATTICE_STEP) * LATTICE_STEP;
    }

    /** @return the value a given fraction of the field lies above */
    private static double quantile(float[] sorted, double share) {
        int index = (int) Math.floor((1.0 - share) * sorted.length);
        return sorted[Math.max(0, Math.min(sorted.length - 1, index))];
    }

    private static void report(OreKind owner, float[] sorted) {
        double sum = 0.0;
        double sumSquares = 0.0;
        for (float value : sorted) {
            sum += value;
            sumSquares += (double) value * value;
        }
        double mean = sum / sorted.length;
        double sigma = Math.sqrt(sumSquares / sorted.length - mean * mean);
        System.out.printf("  %-11s %.0f x %.0f blocks: sigma %.4f, range %.4f .. %.4f%n",
                owner, owner.horizontalSize(), owner.verticalSize(), sigma, sorted[0], sorted[sorted.length - 1]);

        assertTrue(sigma > 0.18 && sigma < 0.40,
                owner + " field is not the width the thresholds were read off: " + sigma);
    }
}
