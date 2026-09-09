// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.generator;

import org.terasology.context.annotation.API;
import org.terasology.cubeworlds.topology.FacePos;
import org.terasology.engine.utilities.procedural.Noise;

/**
 * Reads noise on the sphere rather than in the grid.
 * <p>
 * This is the single reason the seams are continuous. A column is turned into a unit direction and
 * the noise is sampled in <em>three</em> dimensions at {@code direction * radius / featureSize}, so
 * a step of one block along the surface moves the sample by one unit whichever face it happens to
 * be on, and the eight corners are not special: three faces meet there at one and the same point of
 * the sphere.
 * <p>
 * Not thread safe, and not meant to be: create one inside each call to a provider's process method,
 * the way the stock providers create their local buffers. Providers themselves are shared across
 * generation threads.
 */
@API
public final class SphereSampler {

    private final CubeGeometry geometry;
    private final int face;
    private final double[] direction = new double[3];
    private FacePos position;

    public SphereSampler(CubeGeometry geometry, int face) {
        this.geometry = geometry;
        this.face = face;
    }

    /** Moves the sampler to a world column, folding it back onto a real face if it has run off. */
    public void moveTo(double worldX, double worldZ) {
        position = geometry.fold(face, worldX, worldZ);
        geometry.direction(position, direction);
    }

    public FacePos position() {
        return position;
    }

    /** @return the unit direction of the current column. Do not retain: it is reused. */
    public double[] direction() {
        return direction;
    }

    /**
     * @return the sine of the latitude, signed: -1 at the south pole, +1 at the north.
     *         <p>
     *         The sine and not the latitude. The fraction of a sphere's area beyond a parallel is
     *         {@code 1 - sin(phi)}, so the sine is the quantity that is uniform over the sphere,
     *         and it is the only one a threshold can be set on without crowding the poles.
     */
    public double sinLatitude() {
        return Math.max(-1.0, Math.min(1.0, direction[2]));
    }

    /**
     * @param featureSize the size of the noise features, in blocks
     */
    public float read(Noise noise, double featureSize) {
        double k = geometry.radius() / featureSize;
        return noise.noise((float) (direction[0] * k), (float) (direction[1] * k), (float) (direction[2] * k));
    }

    /**
     * Reads at an altitude, so that a feature can vary with height as well as with position.
     * "Up" is radial, so climbing moves the sample outwards.
     */
    public float readAt(Noise noise, double featureSize, double altitude) {
        double k = (geometry.radius() + altitude) / featureSize;
        return noise.noise((float) (direction[0] * k), (float) (direction[1] * k), (float) (direction[2] * k));
    }
}
