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
import org.terasology.engine.world.generation.FacetProvider;
import org.terasology.engine.world.generation.GeneratingRegion;
import org.terasology.engine.world.generation.Produces;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;
import org.terasology.engine.world.generation.facets.ElevationFacet;

import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import java.util.function.Function;

/**
 * Decides which candidate sites carry a ruin, and at what height.
 * <p>
 * <b>Every chunk a ruin touches must reach the same verdict</b>, or a ruin comes out cut in half
 * along a chunk line. So the verdict is not taken from the chunk's facets: it is taken once per
 * site, from a private chain of the same column providers the world is made of, over the whole
 * footprint, and remembered. A chunk then costs a few hashes and a lookup.
 * <p>
 * <b>Not by widening the borders of the chain.</b> That was the first version, and it was right and
 * ruinous: asking the elevation and the biome fifteen blocks around every chunk makes the surface,
 * the climate, the regions and the biome compute nearly four times their area for every chunk of the
 * world, to serve one site in six hundred chunks. {@code ChainCostBreakdownTest} priced it at
 * 6,5 ms a chunk, twice the caves.
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
@Produces(RuinFacet.class)
public class CubeRuinProvider implements FacetProvider {

    /** Blocks over the sea the lowest column of the footprint must stand. */
    public static final int DRY_MARGIN = 2;

    /** Blocks between the highest and the lowest column of the footprint. */
    public static final int MAX_RELIEF = 6;

    private final CubeGeometry geometry;
    private final Function<CubeGeometry, List<FacetProvider>> columnProviders;
    private final Map<Long, Optional<RuinFacet.Ruin>> verdicts = new ConcurrentHashMap<>();
    private RuinLattice lattice;
    private World columns;

    /**
     * @param columnProviders the providers of the elevation and the biome, built afresh: the same
     *         list the world generator puts at the head of its chain, so that the ground judged is
     *         the ground laid
     */
    public CubeRuinProvider(CubeGeometry geometry, Function<CubeGeometry, List<FacetProvider>> columnProviders) {
        this.geometry = geometry;
        this.columnProviders = columnProviders;
    }

    @Override
    public void setSeed(long seed) {
        lattice = new RuinLattice(seed, geometry.faceBlocks());
        WorldBuilder builder = new WorldBuilder(null).setSeaLevel(geometry.seaLevel());
        for (FacetProvider provider : columnProviders.apply(geometry)) {
            builder.addProvider(provider);
        }
        builder.setSeed(seed);
        columns = builder.build();
        verdicts.clear();
    }

    /** @return the candidate sites, or null before {@link #setSeed} */
    public RuinLattice lattice() {
        return lattice;
    }

    @Override
    public void process(GeneratingRegion region) {
        RuinFacet facet = new RuinFacet(region.getRegion(), region.getBorderForFacet(RuinFacet.class));
        BlockRegion area = region.getRegion();
        for (RuinLattice.Site site : lattice.sitesNear(area.minX(), area.minZ(), area.maxX(), area.maxZ(),
                RuinShape.RADIUS)) {
            judge(site).filter(ruin -> reaches(ruin, area)).ifPresent(facet::add);
        }
        region.setRegionFacet(RuinFacet.class, facet);
    }

    /** The verdict on a site, taken once and remembered: the same answer for every chunk that asks. */
    public Optional<RuinFacet.Ruin> judge(RuinLattice.Site site) {
        long key = ((long) site.x() << 32) ^ (site.z() & 0xffffffffL);
        return verdicts.computeIfAbsent(key, k -> Optional.ofNullable(survey(site)));
    }

    private RuinFacet.Ruin survey(RuinLattice.Site site) {
        int r = RuinShape.RADIUS;
        Region footprint = columns.getWorldData(new BlockRegion(site.x() - r, geometry.seaLevel(), site.z() - r)
                .setSize(2 * r + 1, 1, 2 * r + 1));
        BiomeFacet biomes = footprint.getFacet(BiomeFacet.class);
        RuinCulture culture = RuinCulture.of(biomes.getWorld(site.x(), site.z()));
        if (culture == null) {
            return null;
        }
        ElevationFacet elevation = footprint.getFacet(ElevationFacet.class);
        int lowest = Integer.MAX_VALUE;
        int highest = Integer.MIN_VALUE;
        for (int dz = -r; dz <= r; dz++) {
            for (int dx = -r; dx <= r; dx++) {
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
