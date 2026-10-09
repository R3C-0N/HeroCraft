// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.generator.facets;

import org.terasology.cubeworlds.generator.OreKind;
import org.terasology.engine.world.block.BlockRegionc;
import org.terasology.engine.world.generation.Border3D;
import org.terasology.engine.world.generation.facets.base.SparseObjectFacet3D;

/**
 * Which cells of the rock are ore, and which ore.
 * <p>
 * Sparse, where {@link CaveFacet} is dense, and the two are right for opposite reasons. The caves
 * take a couple of percent of the rock and the rasterizer asks about every cell, so a flat array is
 * one byte per cell and one read per question. Ore is a few parts per thousand and nothing has to
 * ask cell by cell: the rasterizer walks {@code getRelativeEntries()} and touches two hundred
 * positions instead of sixty-five thousand.
 * <p>
 * It carries the {@link OreKind} and not a {@code Block}, which is what lets the provider decide
 * without a block manager — and therefore what lets the census measure the design with no engine
 * running at all.
 */
public class OreFacet extends SparseObjectFacet3D<OreKind> {

    public OreFacet(BlockRegionc targetRegion, Border3D border) {
        super(targetRegion, border);
    }
}
