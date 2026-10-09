// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.generator.facets;

import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Border3D;
import org.terasology.engine.world.generation.facets.base.BaseFieldFacet2D;

/**
 * The sine of the latitude, signed: -1 at the south pole, +1 at the north.
 * <p>
 * Signed, because the two caps are not the same place: the north is a frozen ocean in plates and the
 * south is one floating slab. The sign is the only thing that tells them apart, so it has to survive
 * all the way to whoever names the biome.
 * <p>
 * The sine and not the latitude — the fraction of a sphere's area beyond a parallel is
 * {@code 1 - sin(phi)}, so the sine is the quantity a threshold can be set on evenly.
 * <p>
 * It lives in a facet rather than being read again downstream so that exactly one place in the chain
 * touches the sphere. Everything after it decides on facets, and stays testable without a geometry.
 */
public class LatitudeFacet extends BaseFieldFacet2D {
    public LatitudeFacet(BlockRegion targetRegion, Border3D border) {
        super(targetRegion, border);
    }
}
