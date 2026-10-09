// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.generator.facetProviders;

import org.terasology.biomesAPI.Biome;
import org.terasology.core.world.CoreBiome;
import org.terasology.core.world.generator.facets.BiomeFacet;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.OreKind;
import org.terasology.herocubeworlds.generator.SphereFieldLattice;
import org.terasology.herocubeworlds.generator.facets.ArcaneMaskFacet;
import org.terasology.herocubeworlds.generator.facets.CaveFacet;
import org.terasology.herocubeworlds.generator.facets.OreFacet;
import org.terasology.herocubeworlds.generator.facets.VolcanicMaskFacet;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.engine.utilities.procedural.BrownianNoise;
import org.terasology.engine.utilities.procedural.Noise;
import org.terasology.engine.utilities.procedural.SimplexNoise;
import org.terasology.engine.world.block.BlockAreac;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Facet;
import org.terasology.engine.world.generation.GeneratingRegion;
import org.terasology.engine.world.generation.Produces;
import org.terasology.engine.world.generation.Requires;
import org.terasology.engine.world.generation.ScalableFacetProvider;
import org.terasology.engine.world.generation.facets.DensityFacet;
import org.terasology.engine.world.generation.facets.ElevationFacet;
import org.terasology.engine.world.generation.facets.base.BaseFieldFacet2D;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * Puts the ore in the rock.
 * <p>
 * <b>A vein is the top of a field, where a gallery is the middle of two.</b> {@code CubeCaveProvider}
 * carves where two independent fields are both near zero, because that set is a curve and a curve is
 * a tunnel. Ore wants lumps, not threads, so it takes one field and the far end of its distribution:
 * the set above a high threshold is a scatter of blobs. Squash the field vertically and a blob
 * becomes a lens, which is the shape of a seam — that, and nothing else, is what separates the coal
 * from the copper.
 * <p>
 * <b>The size of a vein is not a free setting.</b> It is {@code share * horizontal^2 * vertical},
 * and the number of veins in a chunk column is the number of field boxes in it, which does not
 * depend on the share at all. Shrinking a vein does not make an ore rarer; it dices it. All of that
 * is tabulated in {@link OreKind}, against thresholds {@code OreFieldCalibrationTest} measured
 * rather than derived.
 * <p>
 * <b>It requires {@link CaveFacet} and never reads it.</b> That is not an oversight: the annotation
 * is what places this provider after the caves. {@code UpdatePriority.PRIORITY_REQUIRES} is below
 * every update priority, so a provider that merely requires the density is ordered after everything
 * that updates it — and the caves update the density. Without the requirement the ordering would
 * rest on the order of the calls in the world generator, which is not what {@code WorldBuilder}
 * reads, and ore would be marked in cells the galleries were about to take away.
 * <p>
 * <b>Implements {@link ScalableFacetProvider} and must</b>, like the cave provider: the scalable
 * chain drops the others without a word, and the facet would simply never exist.
 */
@Requires({
        @Facet(DensityFacet.class),
        @Facet(ElevationFacet.class),
        @Facet(BiomeFacet.class),
        @Facet(VolcanicMaskFacet.class),
        @Facet(ArcaneMaskFacet.class),
        @Facet(CaveFacet.class)})
@Produces(OreFacet.class)
public class CubeOreProvider implements ScalableFacetProvider {

    /**
     * Spacing of the noise lattice, in blocks. Must divide a face edge.
     * <p>
     * And must match the step {@code OreFieldCalibrationTest} measures on. The thresholds are
     * quantiles of the <em>interpolated</em> field, and interpolation shaves peaks: a different step
     * is a different field, and every share in the table moves with it.
     */
    private static final int LATTICE_STEP = 4;

    /**
     * One independent stream per field, indexed by ore.
     * <p>
     * Derived by an independent constant and never by adding to a single seed: fields derived by
     * neighbouring offsets move together, and ore that moves with other ore comes out in the same
     * places — the kind of fault a player feels and cannot name.
     */
    private static final long[] FIELD_SALT = {
            0x4b19c2e7L, 0x7d35a80fL, 0x5a0e37c9L, 0x2e64f1b3L, 0x58c07d29L,
            0x13ab6e55L, 0x6f92403dL, 0x3c5d8ab1L, 0x21e7b96fL, 0x74f0152dL};

    /** And one per district that has a field of its own, likewise independent. */
    private static final long[] DISTRICT_SALT = {
            0x00000000L, 0x00000000L, 0x00000000L, 0x00000000L, 0x19d3c74bL,
            0x00000000L, 0x00000000L, 0x00000000L, 0x00000000L, 0x62a84e13L};

    /**
     * Where a district of its own begins, and how sharply.
     * <p>
     * The numbers of {@code ENTRANCE_LEVEL} and {@code ENTRANCE_SOFT}, doing the same job: turning a
     * signed field into a permission that is patchy at the size of its features. Gold and diamond
     * are the two ores nothing else places, so these two decide how much prospecting either takes.
     */
    private static final double DISTRICT_LEVEL = 0.50;
    private static final double DISTRICT_SOFT = 0.15;

    /** The soil depth assumed for a column whose biome is not one of ours, which should not happen. */
    private static final float DEFAULT_SOIL = 32;

    private final CubeGeometry geometry;
    private final Noise[] fields = new Noise[OreKind.values().length];
    private final Noise[] districts = new Noise[OreKind.values().length];

    public CubeOreProvider(CubeGeometry geometry) {
        this.geometry = geometry;
    }

    @Override
    public void setSeed(long seed) {
        for (OreKind ore : OreKind.values()) {
            // Three octaves, which is what the thresholds were measured on. One octave is half again
            // as wide, and every share in the table would roughly double.
            fields[ore.ordinal()] = new BrownianNoise(new SimplexNoise(seed ^ FIELD_SALT[ore.ordinal()]), 3);
            if (ore.district() == OreKind.District.OWN_FIELD) {
                districts[ore.ordinal()] = new BrownianNoise(new SimplexNoise(seed ^ DISTRICT_SALT[ore.ordinal()]), 2);
            }
        }
    }

    @Override
    public void process(GeneratingRegion region, float scale) {
        OreFacet ore = new OreFacet(region.getRegion(), region.getBorderForFacet(OreFacet.class));

        // No ore in the distance, for the reason the caves give: a level-of-detail cell stands for
        // several blocks, so a vein of seven is below the resolution and would come out as a scatter
        // of single cells, in terrain nobody can walk to in any case.
        if (scale > 1f) {
            region.setRegionFacet(OreFacet.class, ore);
            return;
        }
        int face = geometry.faceOfRegion(region.getRegion(), scale);
        if (face == CubePattern.NO_FACE) {
            region.setRegionFacet(OreFacet.class, ore);
            return;
        }

        DensityFacet density = region.getRegionFacet(DensityFacet.class);
        ElevationFacet elevation = region.getRegionFacet(ElevationFacet.class);
        BiomeFacet biomes = region.getRegionFacet(BiomeFacet.class);
        VolcanicMaskFacet volcanic = region.getRegionFacet(VolcanicMaskFacet.class);
        ArcaneMaskFacet arcane = region.getRegionFacet(ArcaneMaskFacet.class);

        BlockRegion area = ore.getWorldRegion();
        BlockRegion rock = density.getWorldRegion();

        List<OreKind> candidates = candidates(area, highestGround(elevation, area));
        if (candidates.isEmpty()) {
            // The commonest outcome by a distance: open sky, open sea, or a depth no band reaches.
            // Leaving before the lattice is built is what keeps that case free.
            region.setRegionFacet(OreFacet.class, ore);
            return;
        }

        int[] fieldOf = new int[OreKind.values().length];
        int[] districtOf = new int[OreKind.values().length];
        SphereFieldLattice lattice = prepare(candidates, fieldOf, districtOf, face, rock);

        int[] tops = new int[candidates.size()];
        int[] bottoms = new int[candidates.size()];
        for (int z = area.minZ(); z <= area.maxZ(); z++) {
            for (int x = area.minX(); x <= area.maxX(); x++) {
                float surface = read(elevation, x, z);
                CoreBiome biome = biomeAt(biomes, x, z);
                float soil = biome == null ? DEFAULT_SOIL : biome.rockDepth();

                int lowest = Integer.MAX_VALUE;
                int highest = Integer.MIN_VALUE;
                for (int i = 0; i < candidates.size(); i++) {
                    OreKind kind = candidates.get(i);
                    double opening = opening(kind, lattice, districtOf, biome, volcanic, arcane, x, z);
                    int top = kind.coreTopAt(surface);
                    int height = bandHeight(kind, top, opening);
                    double middle = (top + kind.coreBottom()) * 0.5;
                    tops[i] = Math.min(area.maxY(), (int) Math.round(middle + height * 0.5));
                    bottoms[i] = Math.max(area.minY(), (int) Math.round(middle - height * 0.5));
                    lowest = Math.min(lowest, bottoms[i]);
                    highest = Math.max(highest, tops[i]);
                }

                for (int y = lowest; y <= highest; y++) {
                    if (!rock.contains(x, y, z)) {
                        continue;
                    }
                    // One test for three facts: solid at all; not a gallery, the caves having written
                    // a negative density where they carved; and rock rather than the soil over it.
                    if (density.getWorld(x, y, z) <= soil) {
                        continue;
                    }
                    for (int i = 0; i < candidates.size(); i++) {
                        if (y < bottoms[i] || y > tops[i]) {
                            continue;
                        }
                        OreKind kind = candidates.get(i);
                        if (lattice.volume(fieldOf[kind.ordinal()], x, y, z) > kind.threshold()) {
                            ore.setWorld(x, y, z, kind);
                            // Richest first, so the first claim is the one worth finding.
                            break;
                        }
                    }
                }
            }
        }

        region.setRegionFacet(OreFacet.class, ore);
    }

    /**
     * Builds the lattice with a field per candidate that owns one, and a column field per district
     * that has one.
     * <p>
     * Only the candidates, and that is where the cost of this provider is decided: a chunk is
     * sixty-four blocks tall and most bands miss it entirely, so a field that cannot reach this
     * region is never evaluated. Registering all nine everywhere would more than double what the
     * galleries already pay for noise.
     */
    private SphereFieldLattice prepare(List<OreKind> candidates, int[] fieldOf, int[] districtOf,
                                       int face, BlockRegion rock) {
        Arrays.fill(fieldOf, -1);
        Arrays.fill(districtOf, -1);
        SphereFieldLattice lattice = new SphereFieldLattice(geometry, LATTICE_STEP);
        for (OreKind kind : candidates) {
            register(lattice, fieldOf, kind);
            if (kind.district() == OreKind.District.OWN_FIELD) {
                districtOf[kind.ordinal()] = lattice.addColumnField(
                        districts[kind.ordinal()], kind.districtSize());
            }
        }
        lattice.prepare(face, rock);
        return lattice;
    }

    private void register(SphereFieldLattice lattice, int[] fieldOf, OreKind kind) {
        if (fieldOf[kind.ordinal()] < 0) {
            fieldOf[kind.ordinal()] = lattice.addVolumeField(
                    fields[kind.ordinal()], kind.horizontalSize(), kind.verticalSize());
        }
    }

    /** @return the ores whose band can reach this region at all, richest first */
    private static List<OreKind> candidates(BlockRegion area, float highestGround) {
        List<OreKind> candidates = new ArrayList<>();
        for (OreKind kind : OreKind.values()) {
            if (kind.coreBottom() <= area.maxY() && kind.coreTopAt(highestGround) >= area.minY()) {
                candidates.add(kind);
            }
        }
        // Declaration order runs from the commonest to the most precious, so reversing it puts the
        // rare ore first: where two bands cross, the rarer one is the find worth having.
        candidates.sort((a, b) -> b.ordinal() - a.ordinal());
        return candidates;
    }

    /**
     * How tall the band is here.
     * <p>
     * Its edge height where the district is absent, its full height at the heart of one, and it
     * always closes towards the band's own middle — one rule for a band pinned to a depth and a band
     * that follows the ground.
     * <p>
     * Never zero. A hard edge would make a region the only place an ore exists, and lapis is what
     * mithril is alloyed with: the arcane region would become a prerequisite for a fantastic metal,
     * which is the one thing the seven base tiers must not be to one another.
     */
    private static int bandHeight(OreKind kind, int top, double opening) {
        if (kind.edgeHeight() == OreKind.FULL) {
            return top - kind.coreBottom();
        }
        double core = top - kind.coreBottom();
        return (int) Math.round(kind.edgeHeight() + (core - kind.edgeHeight()) * opening);
    }

    private double opening(OreKind kind, SphereFieldLattice lattice, int[] districtOf, CoreBiome biome,
                           VolcanicMaskFacet volcanic, ArcaneMaskFacet arcane, int x, int z) {
        switch (kind.district()) {
            case OWN_FIELD:
                return clamp((lattice.column(districtOf[kind.ordinal()], x, z) - DISTRICT_LEVEL) / DISTRICT_SOFT);
            case ARCANE:
                return clamp(read(arcane, x, z) / CubeBiomeProvider.ARCANE_MASK);
            case VOLCANIC:
                // The mask and not the biome. A volcanic biome wants land as well, and a lava sea has
                // the same rock under it either way.
                return clamp(read(volcanic, x, z) / CubeBiomeProvider.VOLCANIC_MASK);
            case ABYSS:
                return biome == CoreBiome.ABYSS ? 1.0 : 0.0;
            case WOODLAND:
                return woodland(biome) ? 1.0 : 0.0;
            default:
                return 1.0;
        }
    }

    private static boolean woodland(CoreBiome biome) {
        return biome == CoreBiome.FOREST || biome == CoreBiome.TAIGA
                || biome == CoreBiome.SWAMP || biome == CoreBiome.MIASMA;
    }

    /** Clamped to the facet, which may be narrower than the region: borders differ between facets. */
    private static float read(BaseFieldFacet2D facet, int x, int z) {
        BlockAreac area = facet.getWorldArea();
        return facet.getWorld(
                Math.max(area.minX(), Math.min(area.maxX(), x)),
                Math.max(area.minY(), Math.min(area.maxY(), z)));
    }

    private static CoreBiome biomeAt(BiomeFacet facet, int x, int z) {
        BlockAreac area = facet.getWorldArea();
        Biome biome = facet.getWorld(
                Math.max(area.minX(), Math.min(area.maxX(), x)),
                Math.max(area.minY(), Math.min(area.maxY(), z)));
        return biome instanceof CoreBiome ? (CoreBiome) biome : null;
    }

    private static float highestGround(ElevationFacet elevation, BlockRegion area) {
        float highest = Float.NEGATIVE_INFINITY;
        for (int z = area.minZ(); z <= area.maxZ(); z++) {
            for (int x = area.minX(); x <= area.maxX(); x++) {
                highest = Math.max(highest, read(elevation, x, z));
            }
        }
        return highest;
    }

    private static double clamp(double value) {
        return Math.max(0.0, Math.min(1.0, value));
    }
}
