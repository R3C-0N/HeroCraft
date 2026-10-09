// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.generator.rasterizers;

import org.terasology.biomesAPI.Biome;
import org.terasology.core.world.CoreBiome;
import org.terasology.cubeworlds.generator.CubeGeometry;

/**
 * What a cell of the cube world is made of, decided without a block manager.
 * <p>
 * This is the decision {@link CubeSolidRasterizer} makes for every cell, pulled out so that
 * something other than a chunk can ask it — the distant terrain summarises columns straight from
 * the facets, and must not grow a second opinion on where the sea stops or which cell is lava. The
 * rasterizer turns the answer into a block; the order of the branches is the rasterizer's own, and
 * it is the whole content of this class.
 */
public enum CubeCell {
    /** Off the world, above or below: no block, and no biome either. */
    OUTSIDE,
    /** Open air inside the world: no block, but the column's biome. */
    AIR,
    /** A carved cell above the lava line: air, with the underground biome. */
    CAVE,
    /** A carved cell at or below the lava line. */
    CAVE_LAVA,
    /** The lowest layer of the world. */
    MANTLE,
    /** Water written by the coarse-scale branch, where a cell straddles the sea level. */
    SEA_STRADDLE,
    /** The biome's surface block. */
    SURFACE,
    /** The biome's block below the surface: soil or rock, by density. */
    BELOW_SURFACE,
    /** The lid of a frozen sea: ice, or the cap's own surface block. */
    SEA_LID,
    /** Open sea. */
    WATER;

    /**
     * @param density the density facet at the cell
     * @param surface whether the surfaces facet marks the cell
     * @param carved whether the cave facet marks the cell
     * @param basePosY the world height of the bottom of the cell, in blocks
     * @param seaLevel the sea level of the world
     * @param scale the size of a cell, in blocks
     */
    public static CubeCell classify(CubeGeometry geometry, float density, boolean surface, boolean carved,
                                    float basePosY, int seaLevel, float scale, Biome biome) {
        if (basePosY < geometry.floorY() || basePosY > geometry.ceilingY()) {
            return OUTSIDE;
        }
        float posY = basePosY + Math.max(0, Math.min(scale, density));
        if (carved) {
            return basePosY <= geometry.lavaY() ? CAVE_LAVA : CAVE;
        }
        if (basePosY < geometry.floorY() + scale) {
            return MANTLE;
        }
        if (posY < seaLevel && basePosY + scale > seaLevel && seaLevel < scale) {
            return SEA_STRADDLE;
        } else if (density > 0 && surface) {
            return SURFACE;
        } else if (density > 0) {
            return BELOW_SURFACE;
        } else if (posY <= seaLevel) {
            if (posY + scale > seaLevel && freezesOver(biome)) {
                return SEA_LID;
            }
            return WATER;
        }
        return AIR;
    }

    /** Whether the sea has a lid here. */
    static boolean freezesOver(Biome biome) {
        return CoreBiome.SNOW == biome || CoreBiome.PACK_ICE == biome || CoreBiome.ICE_SHELF == biome;
    }

    /** Whether the cell holds a liquid: drawn in the distance, but seen through. */
    public boolean isLiquid() {
        return this == CAVE_LAVA || this == WATER || this == SEA_STRADDLE;
    }

    /** Whether a block is written at all. */
    public boolean isFilled() {
        return this != OUTSIDE && this != AIR && this != CAVE;
    }
}
