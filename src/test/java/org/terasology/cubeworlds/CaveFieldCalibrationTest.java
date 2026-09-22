// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.junit.jupiter.api.Test;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.SphereSampler;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.engine.utilities.procedural.BrownianNoise;
import org.terasology.engine.utilities.procedural.Noise;
import org.terasology.engine.utilities.procedural.SimplexNoise;

import java.util.Random;

import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * What the cave thresholds are computed from, measured rather than assumed.
 * <p>
 * A tunnel is the set where two independent fields are both near zero, so its thickness is decided
 * by two numbers: how far the fields wander, and how fast. Both are properties of the noise the
 * engine happens to ship, not of anything this module chose, and both change silently if that noise
 * is ever retuned. Writing them down as literals and hoping is how a generator ends up carving
 * either nothing at all or the whole planet.
 * <p>
 * The numbers this prints are the ones quoted in {@code CubeCaveProvider}. The assertions are wide,
 * because their job is to fail loudly when the ground moves, not to pin a digit.
 */
class CaveFieldCalibrationTest {

    /** The band of the world that has rock in it. */
    private static final int LOWEST = -192;
    private static final int HIGHEST = 120;

    private static final double TUNNEL_H = 96.0;
    private static final double TUNNEL_V = 63.0;

    @Test
    void theFieldsAreAsWideAndAsSteepAsTheThresholdsAssume() {
        CubeGeometry geometry = CubeGeometry.createDefault();

        double[] oneOctave = measure(geometry, new BrownianNoise(new SimplexNoise(0x51A1L), 1));
        double[] threeOctaves = measure(geometry, new BrownianNoise(new SimplexNoise(0x51A2L), 3));
        double[] twoOctaves = measure(geometry, new BrownianNoise(new SimplexNoise(0x51A3L), 2));

        report("1 octave ", oneOctave);
        report("2 octaves", twoOctaves);
        report("3 octaves", threeOctaves);

        // What the provider needs from the one-octave field: a threshold for a target fraction,
        // and the bore that threshold buys. Rayleigh, because a tunnel is the set where the
        // distance of (n1, n2) from the origin is small, and that distance is Rayleigh distributed
        // when the two fields are independent and of equal width.
        double sigma = oneOctave[0];
        // The two systems the provider actually carves, at their own sizes. The gradients scale as
        // one over the feature size, so the minor system's are the main one's times 64/40 across
        // and 42/30 up.
        bore("main  96x63", sigma, 0.014, oneOctave[1], oneOctave[2]);
        bore("minor 60x51", sigma, 0.012, oneOctave[1] * 96 / 60, oneOctave[2] * 63 / 51);
        // The three room tiers, against what the field can actually reach: a level set past the
        // range of the noise is a room that exists nowhere, and nothing on screen would say so.
        double[][] tiers = {{1.5, 2.3}, {2.0, 2.8}, {2.4, 3.0}};
        String[] names = {"chamber ", "hall    ", "cathedral"};
        for (int tier = 0; tier < tiers.length; tier++) {
            System.out.printf("  %s room: level %.1f sigma = %.5f, peak %.1f sigma = %.5f%n",
                    names[tier], tiers[tier][0], tiers[tier][0] * threeOctaves[0],
                    tiers[tier][1], tiers[tier][1] * threeOctaves[0]);
        }

        assertTrue(sigma > 0.34 && sigma < 0.52,
                "the one-octave field is not the width the thresholds were computed for: " + sigma);
        assertTrue(threeOctaves[0] > 0.20 && threeOctaves[0] < 0.36,
                "the three-octave field is not the width the room level assumes: " + threeOctaves[0]);
        assertTrue(oneOctave[2] > oneOctave[1],
                "the field does not vary faster with height than across it, so its zero sets stand "
                        + "on end and two of them meet in a shaft rather than a gallery");
    }

    /** @return sigma, then the per-block gradient horizontally, then radially */
    private static double[] measure(CubeGeometry geometry, Noise noise) {
        int edge = geometry.faceBlocks();
        Random random = new Random(11);
        int samples = 120000;
        double sum = 0.0;
        double sumSquares = 0.0;
        double sumHorizontal = 0.0;
        double sumRadial = 0.0;

        SphereSampler sampler = new SphereSampler(geometry, 0);
        for (int i = 0; i < samples; i++) {
            int face = random.nextInt(CubePattern.FACES);
            sampler = new SphereSampler(geometry, face);
            double u = random.nextDouble() * edge;
            double v = random.nextDouble() * edge;
            double y = LOWEST + random.nextDouble() * (HIGHEST - LOWEST);
            int x = CubePattern.netX(face, 0, edge);
            int z = CubePattern.netY(face, 0, edge);

            sampler.moveTo(x + u, z + v);
            double here = sampler.readAt(noise, TUNNEL_H, TUNNEL_V, y);
            sum += here;
            sumSquares += here * here;

            sampler.moveTo(x + u + 1.0, z + v);
            sumHorizontal += Math.abs(sampler.readAt(noise, TUNNEL_H, TUNNEL_V, y) - here);

            sampler.moveTo(x + u, z + v);
            sumRadial += Math.abs(sampler.readAt(noise, TUNNEL_H, TUNNEL_V, y + 1.0) - here);
        }

        double mean = sum / samples;
        double sigma = Math.sqrt(sumSquares / samples - mean * mean);
        return new double[]{sigma, sumHorizontal / samples, sumRadial / samples};
    }

    /**
     * Prints the gallery a target fraction buys. A bore is only worth reading against the player:
     * a character stands 1.6 blocks tall, so a gallery under two blocks high is a crawl and one
     * under one is a fissure nobody will ever see the inside of.
     */
    private static void bore(String name, double sigma, double fraction,
                             double gradientAcross, double gradientUp) {
        double t = sigma * Math.sqrt(-2.0 * Math.log(1.0 - fraction));
        System.out.printf("  %s: target %.3f of the rock -> threshold %.5f, gallery %.2f wide x %.2f high%n",
                name, fraction, t,
                2.0 * t / (Math.sqrt(2.0) * gradientAcross),
                2.0 * t / (Math.sqrt(2.0) * gradientUp));
    }

    private static void report(String name, double[] measured) {
        System.out.printf("%s: sigma %.4f, gradient %.5f per block across, %.5f per block up%n",
                name, measured[0], measured[1], measured[2]);
    }
}
