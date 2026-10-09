// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.generator.facets;

import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Border3D;
import org.terasology.engine.world.generation.facets.base.BaseFieldFacet2D;

/**
 * How much of the miasma reaches this column, from zero to one.
 * <p>
 * Its own field, on its own noise: see {@link VolcanicMaskFacet}. The miasma is the one that proves
 * the point — given the ordinary swamp's predicate to share, low and flat and waterlogged, there was
 * nothing left of it.
 */
public class MiasmaMaskFacet extends BaseFieldFacet2D {
    public MiasmaMaskFacet(BlockRegion targetRegion, Border3D border) {
        super(targetRegion, border);
    }
}
