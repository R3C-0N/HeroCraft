// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.generator.facetProviders;

import org.terasology.core.world.generator.facets.BiomeFacet;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.RuinCulture;
import org.terasology.herocubeworlds.generator.RuinLattice;
import org.terasology.herocubeworlds.generator.RuinShape;
import org.terasology.herocubeworlds.generator.facets.RuinFacet;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Facet;
import org.terasology.engine.world.generation.FacetBorder;
import org.terasology.engine.world.generation.FacetProvider;
import org.terasology.engine.world.generation.GeneratingRegion;
import org.terasology.engine.world.generation.Produces;
import org.terasology.engine.world.generation.Requires;
import org.terasology.engine.world.generation.facets.ElevationFacet;

/**
 * Decides which candidate sites carry a ruin, and at what height.
 * <p>
 * <b>Every chunk a ruin touches must reach the same verdict</b>, or a ruin comes out cut in half
 * along a chunk line. The verdict therefore reads nothing but the elevation and the biome over the
 * whole footprint, two column facts that do not depend on the region asking; and the border asked
 * for is wide enough to see the whole footprint of any ruin that reaches the region — its centre may
 * lie {@link RuinShape#RADIUS} outside, and its far wall as much again.
 * <p>
 * <b>A site carries a ruin when the ground is dry, flat enough and settled.</b> Dry: every column of
 * the footprint stands {@link #DRY_MARGIN} over the sea, so the court never floods and the foundation
 * never stands in the surf. Flat enough: {@link #MAX_RELIEF} blocks between the highest and the
 * lowest column, which the foundation can fill and the clearing can cut. Settled: the biome at the
 * centre has a {@link RuinCulture}.
 * <p>
 * Not a {@code ScalableFacetProvider}: a ruin is fifteen blocks across, under the resolution of the
 * distant terrain, and nobody walks into a level-of-detail cell.
 */
@Requires({
        @Facet(value = ElevationFacet.class, border = @FacetBorder(sides = CubeRuinProvider.REACH)),
        @Facet(value = BiomeFacet.class, border = @FacetBorder(sides = CubeRuinProvider.REACH))})
@Produces(RuinFacet.class)
public class CubeRuinProvider implements FacetProvider {

    /** Blocks over the sea the lowest column of the footprint must stand. */
    public static final int DRY_MARGIN = 2;

    /** Blocks between the highest and the lowest column of the footprint. */
    public static final int MAX_RELIEF = 6;

    /** How far outside a region the facets must be read: a centre one radius out, its far wall another. */
    static final int REACH = 2 * RuinShape.RADIUS + 1;

    private final CubeGeometry geometry;
    private RuinLattice lattice;

    public CubeRuinProvider(CubeGeometry geometry) {
        this.geometry = geometry;
    }

    @Override
    public void setSeed(long seed) {
        lattice = new RuinLattice(seed, geometry.faceBlocks());
    }

    /** @return the candidate sites, or null before {@link #setSeed} */
    public RuinLattice lattice() {
        return lattice;
    }

    @Override
    public void process(GeneratingRegion region) {
        RuinFacet facet = new RuinFacet(region.getRegion(), region.getBorderForFacet(RuinFacet.class));
        BlockRegion area = region.getRegion();
        ElevationFacet elevation = region.getRegionFacet(ElevationFacet.class);
        BiomeFacet biomes = region.getRegionFacet(BiomeFacet.class);

        for (RuinLattice.Site site : lattice.sitesNear(area.minX(), area.minZ(), area.maxX(), area.maxZ(),
                RuinShape.RADIUS)) {
            RuinFacet.Ruin ruin = judge(site, elevation, biomes);
            if (ruin != null && reaches(ruin, area)) {
                facet.add(ruin);
            }
        }
        region.setRegionFacet(RuinFacet.class, facet);
    }

    private RuinFacet.Ruin judge(RuinLattice.Site site, ElevationFacet elevation, BiomeFacet biomes) {
        RuinCulture culture = RuinCulture.of(biomes.getWorld(site.x(), site.z()));
        if (culture == null) {
            return null;
        }
        int lowest = Integer.MAX_VALUE;
        int highest = Integer.MIN_VALUE;
        for (int dz = -RuinShape.RADIUS; dz <= RuinShape.RADIUS; dz++) {
            for (int dx = -RuinShape.RADIUS; dx <= RuinShape.RADIUS; dx++) {
                int top = topOf(elevation.getWorld(site.x() + dx, site.z() + dz));
                lowest = Math.min(lowest, top);
                highest = Math.max(highest, top);
            }
        }
        if (lowest < geometry.seaLevel() + DRY_MARGIN || highest - lowest > MAX_RELIEF) {
            return null;
        }
        return new RuinFacet.Ruin(site, topOf(elevation.getWorld(site.x(), site.z())), culture);
    }

    /** Whether any block the ruin may write lies in the region's height band. */
    private static boolean reaches(RuinFacet.Ruin ruin, BlockRegion area) {
        return ruin.floorY() - RuinShape.FOUNDATION_DEPTH <= area.maxY()
                && ruin.floorY() + RuinShape.CLEAR_HEIGHT >= area.minY();
    }

    /** The topmost solid cell of a column: a floor of the elevation, as the solid rasterizer lays it. */
    public static int topOf(float elevation) {
        return (int) Math.floor(elevation);
    }
}
