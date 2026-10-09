// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.generator.facets;

import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Border3D;
import org.terasology.engine.world.generation.facets.base.BaseBooleanFieldFacet3D;

/**
 * Which cells the caves took out of the rock.
 * <p>
 * Not the same statement as "this cell is empty", and the difference is the whole reason the facet
 * exists. The density already says empty, and a sea floor is empty: the rasterizer answers an empty
 * cell below the reference level with water, which is right for the sea and would drown every
 * gallery in the world, the sea sitting at 32 and the floor of the world at -192. Carved is a fact
 * about how the cell came to be empty, and nothing else carries it.
 * <p>
 * Dense rather than sparse. A sparse facet pays per entry and the carved cells are a couple of
 * percent of a very large number; a boolean per cell is 65536 bytes for a chunk and is read once
 * per cell by the rasterizer.
 */
public class CaveFacet extends BaseBooleanFieldFacet3D {

    public CaveFacet(BlockRegion targetRegion, Border3D border) {
        super(targetRegion, border);
    }
}
