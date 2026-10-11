// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.generator.facets;

import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Border3D;
import org.terasology.engine.world.generation.facets.base.BaseFieldFacet2D;

/**
 * How much of the seas of lava reaches this column, from zero to one.
 * <p>
 * A mask and not a predicate on the terrain. A region intersected out of climate and shape alone
 * comes out in crumbs — scattered columns that never add up to somewhere one can go. What designates
 * a region is a field of its own, read on the sphere so that a patch of lava is not cut in half at
 * the edge of a face.
 */
public class VolcanicMaskFacet extends BaseFieldFacet2D {
    public VolcanicMaskFacet(BlockRegion targetRegion, Border3D border) {
        super(targetRegion, border);
    }
}
