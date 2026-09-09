// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.generator.facetProviders;

import org.joml.Vector2ic;
import org.terasology.core.world.CoreBiome;
import org.terasology.core.world.generator.facets.BiomeFacet;
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
 * No new biome is declared. A biome earns its place by changing what one does there, and reusing
 * the seven that already have blocks, colours and flora is the honest scope for a first world.
 */
@Produces(BiomeFacet.class)
@Requires({
        @Facet(SeaLevelFacet.class),
        @Facet(ElevationFacet.class),
        @Facet(SurfaceTemperatureFacet.class),
        @Facet(SurfaceHumidityFacet.class)})
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

    @Override
    public void setSeed(long seed) {
    }

    @Override
    public void process(GeneratingRegion region, float scale) {
        SeaLevelFacet seaLevelFacet = region.getRegionFacet(SeaLevelFacet.class);
        ElevationFacet elevationFacet = region.getRegionFacet(ElevationFacet.class);
        SurfaceTemperatureFacet temperatureFacet = region.getRegionFacet(SurfaceTemperatureFacet.class);
        SurfaceHumidityFacet humidityFacet = region.getRegionFacet(SurfaceHumidityFacet.class);

        Border3D border = region.getBorderForFacet(BiomeFacet.class);
        BiomeFacet biomeFacet = new BiomeFacet(region.getRegion(), border);

        int seaLevel = seaLevelFacet.getSeaLevel();

        for (Vector2ic pos : biomeFacet.getRelativeArea()) {
            float temp = temperatureFacet.get(pos);
            float humidity = humidityFacet.get(pos);
            float height = elevationFacet.get(pos) * scale;

            if (height <= seaLevel) {
                biomeFacet.set(pos, CoreBiome.OCEAN);
            } else if (height <= seaLevel + 2) {
                biomeFacet.set(pos, CoreBiome.BEACH);
            } else if (temp <= SNOW_TEMPERATURE) {
                biomeFacet.set(pos, CoreBiome.SNOW);
            } else if (height - seaLevel >= MOUNTAIN_HEIGHT) {
                biomeFacet.set(pos, CoreBiome.MOUNTAINS);
            } else if (temp >= DESERT_TEMPERATURE && humidity < DESERT_HUMIDITY) {
                biomeFacet.set(pos, CoreBiome.DESERT);
            } else if (humidity < FOREST_HUMIDITY) {
                biomeFacet.set(pos, CoreBiome.PLAINS);
            } else {
                biomeFacet.set(pos, CoreBiome.FOREST);
            }
        }
        region.setRegionFacet(BiomeFacet.class, biomeFacet);
    }
}
