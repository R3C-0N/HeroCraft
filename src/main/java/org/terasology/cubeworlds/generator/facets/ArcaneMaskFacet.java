// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.generator.facets;

import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Border3D;
import org.terasology.engine.world.generation.facets.base.BaseFieldFacet2D;

/**
 * How much of the unstable magic reaches this column, from zero to one.
 * <p>
 * Its own field, on its own noise: see {@link VolcanicMaskFacet}.
 */
public class ArcaneMaskFacet extends BaseFieldFacet2D {
    public ArcaneMaskFacet(BlockRegion targetRegion, Border3D border) {
        super(targetRegion, border);
    }
}
