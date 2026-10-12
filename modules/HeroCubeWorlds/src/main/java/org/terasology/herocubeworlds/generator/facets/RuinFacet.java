// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.generator.facets;

import org.terasology.herocubeworlds.generator.RuinCulture;
import org.terasology.herocubeworlds.generator.RuinLattice;
import org.terasology.engine.world.block.BlockRegionc;
import org.terasology.engine.world.generation.Border3D;
import org.terasology.engine.world.generation.facets.base.BaseFacet2D;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * The ruins that reach into a region, whole: where each stands, how high its floor is and who built
 * it.
 * <p>
 * A list and not a field, because there is at most one or two of them to a region and the
 * rasterizer needs the centre, not a cell: the plan is a function of the offset from it.
 */
public class RuinFacet extends BaseFacet2D {

    private final List<Ruin> ruins = new ArrayList<>();

    public RuinFacet(BlockRegionc targetRegion, Border3D border) {
        super(targetRegion, border);
    }

    public void add(Ruin ruin) {
        ruins.add(ruin);
    }

    public List<Ruin> getRuins() {
        return Collections.unmodifiableList(ruins);
    }

    /** One ruin: its site, the height of its floor, and its culture. */
    public static final class Ruin {
        private final RuinLattice.Site site;
        private final int floorY;
        private final RuinCulture culture;

        public Ruin(RuinLattice.Site site, int floorY, RuinCulture culture) {
            this.site = site;
            this.floorY = floorY;
            this.culture = culture;
        }

        public RuinLattice.Site site() {
            return site;
        }

        public int floorY() {
            return floorY;
        }

        public RuinCulture culture() {
            return culture;
        }
    }
}
