// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.generator.facetProviders;

import org.joml.Vector2ic;
import org.terasology.core.world.CoreBiome;
import org.terasology.core.world.generator.facets.BiomeFacet;
import org.terasology.cubeworlds.generator.facets.ArcaneMaskFacet;
import org.terasology.cubeworlds.generator.facets.LatitudeFacet;
import org.terasology.cubeworlds.generator.facets.MiasmaMaskFacet;
import org.terasology.cubeworlds.generator.facets.VolcanicMaskFacet;
import org.terasology.engine.world.generation.Border3D;
import org.terasology.engine.world.generation.Facet;
import org.terasology.engine.world.generation.GeneratingRegion;
import org.terasology.engine.world.generation.Produces;
import org.terasology.engine.world.generation.Requires;
import org.terasology.engine.world.generation.ScalableFacetProvider;
import org.terasology.engine.world.generation.facets.ElevationFacet;
import org.terasology.engine.world.generation.facets.SeaLevelFacet;
import org.terasology.engine.world.generation.facets.SurfaceHumidityFacet;
import org.terasology.engine.world.generation.facets.SurfaceTemperatureFacet;

/**
 * Which biome a column carries.
 * <p>
 * With a real latitude behind the temperature, snow ends up at the poles and desert in a dry belt
 * rather than wherever the noise happened to dip: the thresholds now describe bands instead of
 * blotches.
 * <p>
 * <b>The thresholds are read off the measured distribution, never guessed.</b> Set a priori they
 * opened onto nothing at all — snow was asked for below 0.28 when the whole world bottoms out at
 * 0.342, and mountains above seventy blocks when the relief tops out at forty-one. Both failed in
 * silence, which is why {@code BiomeCensusTest} counts every biome and refuses to pass when one of
 * them exists nowhere. The quantiles behind the numbers below are printed by
 * {@code ClimateDistributionTest}; change the climate and they must be read again.
 * <p>
 * <b>The order of the cascade carries meaning</b>, and it is not the order the biomes were added
 * in. What is tested first wins, so the extreme regions come before the climate: a volcanic cone
 * does not get to be snowy because it is high, and a cap is a cap whether it is ocean or land.
 * <p>
 * Two of the seven regions are absent on purpose. The skies have no column and the deep underground
 * is a volume rather than a column, so {@code CoreBiome.SKY} and {@code CoreBiome.UNDERGROUND} are
 * assigned nowhere in here. See their declarations.
 */
@Produces(BiomeFacet.class)
@Requires({
        @Facet(SeaLevelFacet.class),
        @Facet(ElevationFacet.class),
        @Facet(SurfaceTemperatureFacet.class),
        @Facet(SurfaceHumidityFacet.class),
        @Facet(VolcanicMaskFacet.class),
        @Facet(ArcaneMaskFacet.class),
        @Facet(MiasmaMaskFacet.class),
        @Facet(LatitudeFacet.class)})
public class CubeBiomeProvider implements ScalableFacetProvider {

    /**
     * Just above the warmest polar column measured, 0.460, and below the world's first quartile,
     * 0.462: the caps get snow and the temperate band does not.
     */
    private static final float SNOW_TEMPERATURE = 0.42f;

    /** The top few percent of the relief, whose ninety-fifth percentile sits at 33 blocks. */
    private static final int MOUNTAIN_HEIGHT = 30;

    /** Above the world's median temperature, 0.658, and in its dry quarter, humidity 0.353. */
    private static final float DESERT_TEMPERATURE = 0.62f;
    private static final float DESERT_HUMIDITY = 0.38f;

    /** The wetter half of the land carries trees; the drier half is open ground. */
    private static final float FOREST_HUMIDITY = 0.48f;

    /**
     * The cold band between the snow and the temperate wood: colder than the world's first
     * quartile, warmer than the snow line. A taiga also has to be damp — cold and dry is a bare
     * mountainside, not a conifer forest.
     */
    private static final float TAIGA_TEMPERATURE = 0.52f;
    private static final float TAIGA_HUMIDITY = 0.40f;

    /**
     * A savannah is hot and middling-dry: hotter than the median, but not dry enough to be a
     * desert, which is what leaves it as the band between the two.
     */
    private static final float SAVANNA_TEMPERATURE = 0.70f;

    /**
     * A swamp is low and very wet. Low, and not flat: the cube chain carries no roughness facet, so
     * there is no measure of flatness to ask for.
     * <p>
     * TODO flatness — the stock {@code BiomeProvider} tells a plain from a mountain with
     * {@code SurfaceRoughnessFacet}, which {@code CubeSurfaceProvider} does not produce. Until it
     * does, a swamp is decided on height above the sea alone, and a wet hollow high up will not be
     * one.
     */
    private static final int SWAMP_HEIGHT = 6;
    private static final float SWAMP_HUMIDITY = 0.62f;

    /**
     * How far below the reference level the sea floor has to fall to be the abyss.
     * <p>
     * Read off the low tail of the elevation, not off what a deep sea sounds like: the whole relief
     * spans plus or minus 41 blocks, and its fifth percentile is at -34. Sixty blocks — a plausible
     * enough number for a deep sea — opened onto nothing at all.
     */
    private static final int ABYSS_DEPTH = 32;

    /**
     * Where a cap begins, in sine of latitude — so, as a fraction of the sphere's area, the last
     * {@code 1 - 0.90} of it at each end.
     */
    private static final float CAP_LATITUDE = 0.90f;

    /**
     * How much of a mask a column needs to belong to its region.
     * <p>
     * These are read on the fields <em>after</em> the falloff of {@code CubeRegionProvider}, which
     * is why they look low: the fields spend most of their range near zero by design, and that is
     * what keeps the extremes a few percent of the world. Each sits between the ninety-fifth
     * percentile of its own field and its peak — 0.123 and 0.405 volcanic, 0.128 and 0.359 arcane,
     * 0.166 and 0.447 miasmic — which is what puts each region at a fraction of a percent of the
     * world rather than at nothing or at a tenth of it.
     * <p>
     * Both failures worth remembering happened here. A flat 0.30 across the board asked the
     * volcanic field for more than its peak, and opened the seas of lava nowhere. Then, with the
     * fields rescaled, 0.29 was still above everything the arcane field reached on land — and a
     * region that exists nowhere is not an error, it is a census line reading "nowhere".
     */
    private static final float VOLCANIC_MASK = 0.22f;
    private static final float ARCANE_MASK = 0.20f;
    private static final float MIASMA_MASK = 0.18f;

    @Override
    public void setSeed(long seed) {
    }

    @Override
    public void process(GeneratingRegion region, float scale) {
        SeaLevelFacet seaLevelFacet = region.getRegionFacet(SeaLevelFacet.class);
        ElevationFacet elevationFacet = region.getRegionFacet(ElevationFacet.class);
        SurfaceTemperatureFacet temperatureFacet = region.getRegionFacet(SurfaceTemperatureFacet.class);
        SurfaceHumidityFacet humidityFacet = region.getRegionFacet(SurfaceHumidityFacet.class);
        VolcanicMaskFacet volcanicFacet = region.getRegionFacet(VolcanicMaskFacet.class);
        ArcaneMaskFacet arcaneFacet = region.getRegionFacet(ArcaneMaskFacet.class);
        MiasmaMaskFacet miasmaFacet = region.getRegionFacet(MiasmaMaskFacet.class);
        LatitudeFacet latitudeFacet = region.getRegionFacet(LatitudeFacet.class);

        Border3D border = region.getBorderForFacet(BiomeFacet.class);
        BiomeFacet biomeFacet = new BiomeFacet(region.getRegion(), border);

        int seaLevel = seaLevelFacet.getSeaLevel();

        // World coordinates rather than relative ones, all the way through. The facets read here do
        // not all carry the same border — the tree provider asks for a margin around the biome and
        // nobody asks for one around a mask — and a relative position means a different column in
        // each of them as soon as their borders differ. It costs nothing to be unambiguous.
        for (Vector2ic pos : biomeFacet.getWorldArea()) {
            float temp = temperatureFacet.getWorld(pos);
            float humidity = humidityFacet.getWorld(pos);
            float height = elevationFacet.getWorld(pos) * scale;
            float latitude = latitudeFacet.getWorld(pos);
            float depth = seaLevel - height;
            boolean land = height > seaLevel;

            if (land && volcanicFacet.getWorld(pos) >= VOLCANIC_MASK) {
                // Before every test of temperature: a cone that has lava on it does not hold snow,
                // however high or far north it stands.
                biomeFacet.setWorld(pos.x(), pos.y(), CoreBiome.VOLCANIC);
            } else if (Math.abs(latitude) >= CAP_LATITUDE) {
                // A cap covers its open water as much as its land, and the sign is what says which
                // cap: a frozen ocean in plates to the north, one floating slab to the south.
                // Before the abyss, and not after: the abyss is under the pack ice rather than
                // beside it, and what one crosses at a cap is the lid.
                biomeFacet.setWorld(pos.x(), pos.y(), latitude > 0 ? CoreBiome.PACK_ICE : CoreBiome.ICE_SHELF);
            } else if (depth >= ABYSS_DEPTH) {
                biomeFacet.setWorld(pos.x(), pos.y(), CoreBiome.ABYSS);
            } else if (!land) {
                biomeFacet.setWorld(pos.x(), pos.y(), CoreBiome.OCEAN);
            } else if (height <= seaLevel + 2) {
                biomeFacet.setWorld(pos.x(), pos.y(), CoreBiome.BEACH);
            } else if (arcaneFacet.getWorld(pos) >= ARCANE_MASK) {
                biomeFacet.setWorld(pos.x(), pos.y(), CoreBiome.ARCANE);
            } else if (miasmaFacet.getWorld(pos) >= MIASMA_MASK && humidity >= SWAMP_HUMIDITY) {
                // The mask designates the region and the water is a condition of its scenery, not
                // the other way round.
                biomeFacet.setWorld(pos.x(), pos.y(), CoreBiome.MIASMA);
            } else if (temp <= SNOW_TEMPERATURE) {
                biomeFacet.setWorld(pos.x(), pos.y(), CoreBiome.SNOW);
            } else if (temp <= TAIGA_TEMPERATURE && humidity >= TAIGA_HUMIDITY) {
                biomeFacet.setWorld(pos.x(), pos.y(), CoreBiome.TAIGA);
            } else if (height - seaLevel >= MOUNTAIN_HEIGHT) {
                biomeFacet.setWorld(pos.x(), pos.y(), CoreBiome.MOUNTAINS);
            } else if (temp >= DESERT_TEMPERATURE && humidity < DESERT_HUMIDITY) {
                biomeFacet.setWorld(pos.x(), pos.y(), CoreBiome.DESERT);
            } else if (temp >= SAVANNA_TEMPERATURE && humidity < FOREST_HUMIDITY) {
                biomeFacet.setWorld(pos.x(), pos.y(), CoreBiome.SAVANNA);
            } else if (height <= seaLevel + SWAMP_HEIGHT && humidity >= SWAMP_HUMIDITY) {
                biomeFacet.setWorld(pos.x(), pos.y(), CoreBiome.SWAMP);
            } else if (humidity < FOREST_HUMIDITY) {
                biomeFacet.setWorld(pos.x(), pos.y(), CoreBiome.PLAINS);
            } else {
                biomeFacet.setWorld(pos.x(), pos.y(), CoreBiome.FOREST);
            }
        }
        region.setRegionFacet(BiomeFacet.class, biomeFacet);
    }
}
