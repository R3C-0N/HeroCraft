// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.generator;

import org.terasology.context.annotation.API;
import org.terasology.cubeworlds.projection.ConformalCubeTable;
import org.terasology.cubeworlds.topology.CubeFolding;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.cubeworlds.topology.FacePos;
import org.terasology.engine.world.block.BlockRegionc;
import org.terasology.engine.world.chunks.Chunks;

/**
 * The size and shape of one cube world, shared by everything that needs to agree on it.
 * <p>
 * Put into the context by the world generator, so that generation and rendering read the same
 * numbers from the same object. Two copies of the radius would be two chances to disagree.
 */
@API
public final class CubeGeometry {

    /** Chunks along the edge of a face. 100 gives a radius of 1967 blocks, so 12360 around. */
    public static final int DEFAULT_FACE_CHUNKS = 100;

    /** Reference height. Terrain is measured from here, and so is the sphere's surface. */
    public static final int DEFAULT_SEA_LEVEL = 32;

    /**
     * The bottom of the world, and the last block of it: a layer of mantle stone.
     * <p>
     * On a chunk boundary, and not by luck: -192 is -3 x 64 and 320 is 5 x 64, so the world is
     * exactly eight chunks tall and no chunk is ever half in and half out. Kept here rather than in
     * the rasterizer for the reason this whole class exists — two copies of a number are two chances
     * to disagree, and the build limit reads the same two.
     */
    public static final int DEFAULT_FLOOR_Y = -192;

    /** The top of the world. Nothing is generated above it, and nothing may be built above it. */
    public static final int DEFAULT_CEILING_Y = 319;

    /**
     * The surface of the lava, filling whatever the caves opened below it.
     * <p>
     * A level and not a scattering of pools, for two reasons. A flat surface is what a liquid
     * looks like, so it reads as one without anything having to flow; and it makes the bottom of
     * the world one recognisable place rather than a series of accidents, which is what a region
     * reached on purpose ought to be.
     */
    public static final int DEFAULT_LAVA_Y = -150;

    private final int faceChunks;
    private final int faceBlocks;
    private final double radius;
    private final int seaLevel;
    private final ConformalCubeTable table;

    public CubeGeometry(int faceChunks, int seaLevel, ConformalCubeTable table) {
        this.faceChunks = faceChunks;
        this.faceBlocks = faceChunks * Chunks.SIZE_X;
        this.radius = ConformalCubeTable.radiusFor(faceBlocks);
        this.seaLevel = seaLevel;
        this.table = table;
    }

    public static CubeGeometry createDefault() {
        return new CubeGeometry(DEFAULT_FACE_CHUNKS, DEFAULT_SEA_LEVEL, ConformalCubeTable.build());
    }

    public int faceChunks() {
        return faceChunks;
    }

    public int faceBlocks() {
        return faceBlocks;
    }

    /** In blocks. Not a setting: it is the face edge over four times K. */
    public double radius() {
        return radius;
    }

    public int seaLevel() {
        return seaLevel;
    }

    /** The lowest block of the world, which is mantle stone. Nothing exists below it. */
    public int floorY() {
        return DEFAULT_FLOOR_Y;
    }

    /** The highest block of the world. Nothing is generated or built above it. */
    public int ceilingY() {
        return DEFAULT_CEILING_Y;
    }

    /** The surface of the lava that fills the deepest galleries. */
    public int lavaY() {
        return DEFAULT_LAVA_Y;
    }

    public ConformalCubeTable table() {
        return table;
    }

    /** @return the face a world column belongs to, or {@link CubePattern#NO_FACE} for a dead slot */
    public int faceOf(int worldX, int worldZ) {
        return CubePattern.faceOf(worldX, worldZ, faceBlocks);
    }

    /**
     * The face a region is being generated for.
     * <p>
     * Read at the centre of the region and never at a corner of a bordered facet area: a border
     * already reaches past the seam, so it would answer with the face next door, or with a dead
     * slot. The coordinates of a level-of-detail region are in scaled units, so they are brought
     * back to real world columns first.
     */
    public int faceOfRegion(BlockRegionc region, float scale) {
        int x = (int) ((region.minX() + Chunks.SIZE_X / 2.0) * scale);
        int z = (int) ((region.minZ() + Chunks.SIZE_Z / 2.0) * scale);
        return faceOf(x, z);
    }

    /**
     * @return the position of a world column, read from the face it is being generated for. A
     *         column past the edge of that face is folded onto the face next door, which is the
     *         whole reason a facet border can cross a seam without tearing.
     */
    public FacePos fold(int fromFace, double worldX, double worldZ) {
        return CubeFolding.foldNet(fromFace, worldX, worldZ, faceBlocks);
    }

    /** @param dest receives the unit direction, in planet space */
    public void direction(FacePos pos, double[] dest) {
        table.direction(pos.face(),
                2.0 * pos.u() / faceBlocks - 1.0,
                2.0 * pos.v() / faceBlocks - 1.0,
                dest);
    }

    /** @return the tangential scale factor at a position: the size of a block, in blocks */
    public double scaleFactor(FacePos pos) {
        return table.lambda(2.0 * pos.u() / faceBlocks - 1.0, 2.0 * pos.v() / faceBlocks - 1.0);
    }

    /** @return the world column at the centre of a face, as {x, z} */
    public int[] faceCentre(int face) {
        return new int[]{
                CubePattern.netX(face, faceBlocks / 2, faceBlocks),
                CubePattern.netY(face, faceBlocks / 2, faceBlocks)
        };
    }

    /** Distance over the sphere, as a chord: it differs from the arc by d squared over 24 R squared. */
    public double distance(double[] directionA, double[] directionB) {
        double dx = directionA[0] - directionB[0];
        double dy = directionA[1] - directionB[1];
        double dz = directionA[2] - directionB[2];
        return Math.sqrt(dx * dx + dy * dy + dz * dz) * radius;
    }

    /** How far one can see from a given height above the reference level. */
    public double horizon(double heightAboveSeaLevel) {
        double h = Math.max(0.0, heightAboveSeaLevel);
        return radius * Math.acos(radius / (radius + h));
    }
}
