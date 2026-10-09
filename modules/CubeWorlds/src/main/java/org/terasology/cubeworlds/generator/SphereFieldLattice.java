// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.generator;

import com.google.common.base.Preconditions;
import org.terasology.context.annotation.API;
import org.terasology.engine.utilities.procedural.Noise;
import org.terasology.engine.world.block.BlockRegionc;
import org.terasology.math.TeraMath;

import java.util.ArrayList;
import java.util.List;

/**
 * Noise read on the sphere, sampled on a lattice and interpolated between.
 * <p>
 * {@link SphereSampler} answers one point at a time, and a volume pass cannot afford that: a chunk
 * is 65536 cells, and a handful of fields over it comes to half a million evaluations of simplex.
 * The engine's own {@code SubSampledNoise} exists for exactly this and is of no use here — it steps
 * along the axes of the grid, and the projection onto the sphere is not linear along them. So the
 * sub-sampling is done again, on the sphere.
 * <p>
 * Two things about it are load-bearing.
 * <p>
 * <b>The lattice is anchored to the face, not to the net.</b> Nodes sit at multiples of the step in
 * world coordinates, and a face origin is itself a multiple of the step, so a node on a seam plane
 * is at the same face-local place seen from either side — and folding sends both to one and the
 * same point of the sphere, so both read the same number. Anchor it anywhere else and the field
 * agrees across the ten seams that are adjacent in the net and breaks on the other fourteen, which
 * is a fault no small test world would ever show. Hence the check that the step divides a face.
 * <p>
 * <b>What is stored is the signed noise.</b> Never a folded value, never a decision. Folding after
 * interpolating keeps the zero level exactly where it was; interpolating a folded value flattens
 * the ridge between two nodes, and a tunnel carved from it comes out pinched.
 * <p>
 * Not thread safe, and not meant to be: build one inside each call to a provider's process method,
 * the way {@link SphereSampler} is built. Providers are shared across generation threads.
 */
@API
public final class SphereFieldLattice {

    private final CubeGeometry geometry;
    private final int step;
    private final List<double[]> volumeFields = new ArrayList<>();
    private final List<Noise> volumeNoises = new ArrayList<>();
    private final List<Double> columnFields = new ArrayList<>();
    private final List<Noise> columnNoises = new ArrayList<>();

    private int originX;
    private int originY;
    private int originZ;
    private int countX;
    private int countY;
    private int countZ;
    private float[] volumeKeys;
    private float[] columnKeys;

    /**
     * @param step spacing of the lattice in blocks; must divide a face edge
     */
    public SphereFieldLattice(CubeGeometry geometry, int step) {
        Preconditions.checkArgument(step > 0 && geometry.faceBlocks() % step == 0,
                "a lattice step of %s does not divide a face of %s blocks, so the nodes on a seam "
                        + "would not line up", step, geometry.faceBlocks());
        this.geometry = geometry;
        this.step = step;
    }

    /** @return the index of a field that varies in three dimensions */
    public int addVolumeField(Noise noise, double horizontalSize, double verticalSize) {
        volumeNoises.add(noise);
        volumeFields.add(new double[]{horizontalSize, verticalSize});
        return volumeNoises.size() - 1;
    }

    /** @return the index of a field that depends on the column alone */
    public int addColumnField(Noise noise, double featureSize) {
        columnNoises.add(noise);
        columnFields.add(featureSize);
        return columnNoises.size() - 1;
    }

    /**
     * Evaluates every field on the lattice covering a region, in world blocks.
     * <p>
     * One {@code moveTo} per node column, shared by all the fields: folding a column onto its face
     * and reading the conformal table is the expensive half, and it does not depend on altitude or
     * on which field is being read.
     */
    public void prepare(int face, BlockRegionc region) {
        originX = Math.floorDiv(region.minX(), step) * step;
        originY = Math.floorDiv(region.minY(), step) * step;
        originZ = Math.floorDiv(region.minZ(), step) * step;
        countX = (region.maxX() - originX) / step + 2;
        countY = (region.maxY() - originY) / step + 2;
        countZ = (region.maxZ() - originZ) / step + 2;

        int volumeCount = volumeNoises.size();
        int columnCount = columnNoises.size();
        volumeKeys = new float[volumeCount * countX * countY * countZ];
        columnKeys = new float[columnCount * countX * countZ];

        SphereSampler sampler = new SphereSampler(geometry, face);
        for (int iz = 0; iz < countZ; iz++) {
            for (int ix = 0; ix < countX; ix++) {
                sampler.moveTo(originX + ix * (double) step, originZ + iz * (double) step);
                for (int field = 0; field < columnCount; field++) {
                    columnKeys[columnIndex(field, ix, iz)] =
                            sampler.read(columnNoises.get(field), columnFields.get(field));
                }
                for (int iy = 0; iy < countY; iy++) {
                    double altitude = originY + iy * (double) step;
                    for (int field = 0; field < volumeCount; field++) {
                        double[] sizes = volumeFields.get(field);
                        volumeKeys[volumeIndex(field, ix, iy, iz)] =
                                sampler.readAt(volumeNoises.get(field), sizes[0], sizes[1], altitude);
                    }
                }
            }
        }
    }

    /** @return the interpolated value of a volume field at a world position */
    public float volume(int field, int worldX, int worldY, int worldZ) {
        int fx = worldX - originX;
        int fy = worldY - originY;
        int fz = worldZ - originZ;
        int ix = fx / step;
        int iy = fy / step;
        int iz = fz / step;
        float tx = (fx - ix * step) / (float) step;
        float ty = (fy - iy * step) / (float) step;
        float tz = (fz - iz * step) / (float) step;
        return TeraMath.triLerp(
                volumeKeys[volumeIndex(field, ix, iy, iz)],
                volumeKeys[volumeIndex(field, ix + 1, iy, iz)],
                volumeKeys[volumeIndex(field, ix, iy + 1, iz)],
                volumeKeys[volumeIndex(field, ix + 1, iy + 1, iz)],
                volumeKeys[volumeIndex(field, ix, iy, iz + 1)],
                volumeKeys[volumeIndex(field, ix + 1, iy, iz + 1)],
                volumeKeys[volumeIndex(field, ix, iy + 1, iz + 1)],
                volumeKeys[volumeIndex(field, ix + 1, iy + 1, iz + 1)],
                tx, ty, tz);
    }

    /** @return the interpolated value of a column field at a world column */
    public float column(int field, int worldX, int worldZ) {
        int fx = worldX - originX;
        int fz = worldZ - originZ;
        int ix = fx / step;
        int iz = fz / step;
        float tx = (fx - ix * step) / (float) step;
        float tz = (fz - iz * step) / (float) step;
        return TeraMath.biLerp(
                columnKeys[columnIndex(field, ix, iz)],
                columnKeys[columnIndex(field, ix + 1, iz)],
                columnKeys[columnIndex(field, ix, iz + 1)],
                columnKeys[columnIndex(field, ix + 1, iz + 1)],
                tx, tz);
    }

    private int volumeIndex(int field, int ix, int iy, int iz) {
        return field * countX * countY * countZ + ix + countX * (iy + countY * iz);
    }

    private int columnIndex(int field, int ix, int iz) {
        return field * countX * countZ + ix + countX * iz;
    }
}
