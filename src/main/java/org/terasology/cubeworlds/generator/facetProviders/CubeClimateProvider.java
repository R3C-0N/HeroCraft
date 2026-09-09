// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.generator.facetProviders;

import org.joml.Vector2ic;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.SphereSampler;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.engine.utilities.procedural.BrownianNoise;
import org.terasology.engine.utilities.procedural.Noise;
import org.terasology.engine.utilities.procedural.SimplexNoise;
import org.terasology.engine.world.block.BlockAreac;
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
 * Temperature and humidity, taken from the real latitude.
 * <p>
 * Temperature is dominated by the band of latitude, with altitude and a background field behind it,
 * in the proportions the Veloren fork settled on. Two things matter about how they are combined.
 * <p>
 * The band is built on the <em>sine</em> of the latitude, because that is the quantity that spreads
 * evenly over a sphere; using the latitude itself would crowd the bands towards the poles.
 * <p>
 * And the lapse rate is applied <em>outside</em> the weighted mixture, not as one more term in it.
 * Altitude already enters the mixture as a broad tendency — cold where the land is high — and that
 * is a different statement from "cold because this particular summit is two hundred blocks up".
 * Only the second one puts snow on an equatorial peak, and only an explicit gradient says it.
 */
@Produces({SurfaceTemperatureFacet.class, SurfaceHumidityFacet.class})
@Requires({@Facet(ElevationFacet.class), @Facet(SeaLevelFacet.class)})
public class CubeClimateProvider implements ScalableFacetProvider {

    /** How much the band of latitude, the altitude and the background field each weigh. */
    private static final double WEIGHT_LATITUDE = 3.0;
    private static final double WEIGHT_ALTITUDE = 1.5;
    private static final double WEIGHT_NOISE = 0.75;

    /** How far the noise is allowed to make a band's edge wander, in units of sine of latitude. */
    private static final double BAND_WAVER = 0.10;

    /** Cooling per block of altitude, on the zero to one scale of the facet. */
    private static final double LAPSE_RATE = 0.0012;

    /** Altitude at which the broad altitude tendency saturates, in blocks above the reference. */
    private static final double ALTITUDE_SPAN = 160.0;

    private static final double CLIMATE_SIZE = 3400.0;
    private static final double HUMIDITY_SIZE = 900.0;

    /** Above this temperature the air starts drying out. */
    private static final double EVAPORATION_THRESHOLD = 0.60;
    private static final double EVAPORATION_SLOPE = 1.2;
    private static final double EVAPORATION_FLOOR = 0.80;

    private final CubeGeometry geometry;
    private Noise bandNoise;
    private Noise backgroundNoise;
    private Noise humidityNoise;

    public CubeClimateProvider(CubeGeometry geometry) {
        this.geometry = geometry;
    }

    @Override
    public void setSeed(long seed) {
        bandNoise = new BrownianNoise(new SimplexNoise(seed ^ 0x2ab4f19dL), 4);
        backgroundNoise = new BrownianNoise(new SimplexNoise(seed ^ 0x7c1de503L), 3);
        humidityNoise = new BrownianNoise(new SimplexNoise(seed ^ 0x39a0be21L), 6);
    }

    @Override
    public void process(GeneratingRegion region, float scale) {
        SurfaceTemperatureFacet temperature = new SurfaceTemperatureFacet(
                region.getRegion(), region.getBorderForFacet(SurfaceTemperatureFacet.class));
        SurfaceHumidityFacet humidity = new SurfaceHumidityFacet(
                region.getRegion(), region.getBorderForFacet(SurfaceHumidityFacet.class));
        ElevationFacet elevation = region.getRegionFacet(ElevationFacet.class);
        int seaLevel = region.getRegionFacet(SeaLevelFacet.class).getSeaLevel();

        int face = geometry.faceOfRegion(region.getRegion(), scale);
        BlockAreac area = temperature.getWorldArea();
        if (face == CubePattern.NO_FACE) {
            for (Vector2ic pos : area) {
                temperature.setWorld(pos.x(), pos.y(), 0.5f);
                humidity.setWorld(pos.x(), pos.y(), 0.5f);
            }
            region.setRegionFacet(SurfaceTemperatureFacet.class, temperature);
            region.setRegionFacet(SurfaceHumidityFacet.class, humidity);
            return;
        }

        SphereSampler sampler = new SphereSampler(geometry, face);
        for (Vector2ic pos : area) {
            sampler.moveTo(pos.x() * scale, pos.y() * scale);

            double band = 1.0 - Math.abs(sampler.sinLatitude() + sampler.read(bandNoise, CLIMATE_SIZE) * BAND_WAVER);
            band = Math.max(0.0, Math.min(1.0, band));

            double height = elevation.getWorld(pos) * scale - seaLevel;
            double altitudeTendency = 1.0 - Math.max(0.0, Math.min(1.0, height / ALTITUDE_SPAN));
            double background = (sampler.read(backgroundNoise, CLIMATE_SIZE) + 1.0) * 0.5;

            double mixed = (WEIGHT_LATITUDE * band
                    + WEIGHT_ALTITUDE * altitudeTendency
                    + WEIGHT_NOISE * background)
                    / (WEIGHT_LATITUDE + WEIGHT_ALTITUDE + WEIGHT_NOISE);

            double temp = mixed - LAPSE_RATE * Math.max(0.0, height);
            temp = Math.max(0.0, Math.min(1.0, temp));
            temperature.setWorld(pos.x(), pos.y(), (float) temp);

            double wet = (sampler.read(humidityNoise, HUMIDITY_SIZE) + 1.0) * 0.5;
            wet *= evaporation(temp);
            humidity.setWorld(pos.x(), pos.y(), (float) Math.max(0.0, Math.min(1.0, wet)));
        }

        region.setRegionFacet(SurfaceTemperatureFacet.class, temperature);
        region.setRegionFacet(SurfaceHumidityFacet.class, humidity);
    }

    /** Hot air holds less of what it has already lost: the drying factor above the threshold. */
    private static double evaporation(double temp) {
        double excess = Math.max(0.0, temp - EVAPORATION_THRESHOLD);
        return Math.max(EVAPORATION_FLOOR, 1.0 - excess / EVAPORATION_SLOPE);
    }
}
