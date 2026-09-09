// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.generator;

import org.joml.Vector3fc;
import org.terasology.core.world.generator.facetProviders.DefaultFloraProvider;
import org.terasology.core.world.generator.facetProviders.DefaultTreeProvider;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.core.world.generator.facetProviders.SurfaceToDensityProvider;
import org.terasology.core.world.generator.rasterizers.FloraRasterizer;
import org.terasology.core.world.generator.rasterizers.SunlightRasterizer;
import org.terasology.core.world.generator.rasterizers.TreeRasterizer;
import org.terasology.cubeworlds.generator.facetProviders.CubeBiomeProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.cubeworlds.generator.rasterizers.CubeSolidRasterizer;
import org.terasology.engine.context.Context;
import org.terasology.engine.core.SimpleUri;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.logic.spawner.FixedSpawner;
import org.terasology.engine.registry.In;
import org.terasology.engine.world.generation.BaseFacetedWorldGenerator;
import org.terasology.engine.world.generation.WorldBuilder;
import org.terasology.engine.world.generator.RegisterWorldGenerator;
import org.terasology.engine.world.generator.plugin.WorldGeneratorPluginLibrary;

/**
 * A world laid out as the net of a cube.
 * <p>
 * The six faces sit as a cross in the world's XZ plane, and Y is the depth of whichever face one
 * stands on. A face is therefore an ordinary flat heightmap, which is why every piece of the stock
 * chain below — density, surfaces, sunlight, flora, trees — is reused rather than rewritten. What is
 * new is only where the noise is read: on the sphere, through a unit direction, which is what makes
 * the seams continuous and gives the world a real latitude.
 */
@RegisterWorldGenerator(id = "cubeworld", displayName = "Cube World",
        description = "Six faces of a cube, laid out as a net and rendered as a sphere")
public class CubeWorldGenerator extends BaseFacetedWorldGenerator {

    @In
    private WorldGeneratorPluginLibrary worldGeneratorPluginLibrary;

    @In
    private Context context;

    private final CubeGeometry geometry = CubeGeometry.createDefault();
    private final FixedSpawner spawner;

    public CubeWorldGenerator(SimpleUri uri) {
        super(uri);
        // The equatorial band, at the middle of a face: as far as one can get from the seven seams
        // that face a dead slot, and therefore from the void one would otherwise be looking at.
        int[] centre = geometry.faceCentre(1);
        spawner = new FixedSpawner(centre[0], centre[1]);
    }

    public CubeGeometry getGeometry() {
        return geometry;
    }

    @Override
    public void initialize() {
        // Published before the render graph is built, so generation and rendering read one and the
        // same radius from one and the same object.
        if (context != null) {
            context.put(CubeGeometry.class, geometry);
        }
        super.initialize();
    }

    @Override
    public Vector3fc getSpawnPosition(EntityRef entity) {
        return spawner.getSpawnPosition(getWorld(), entity);
    }

    @Override
    protected WorldBuilder createWorld() {
        return new WorldBuilder(worldGeneratorPluginLibrary)
                .setSeaLevel(geometry.seaLevel())
                .addProvider(new SeaLevelProvider(geometry.seaLevel()))
                .addProvider(new CubeSurfaceProvider(geometry))
                .addProvider(new CubeClimateProvider(geometry))
                .addProvider(new CubeBiomeProvider())
                .addProvider(new SurfaceToDensityProvider())
                .addProvider(new DefaultFloraProvider())
                .addProvider(new DefaultTreeProvider())
                .addRasterizer(new CubeSolidRasterizer(geometry))
                .addPlugins()
                .addRasterizer(new FloraRasterizer())
                .addRasterizer(new TreeRasterizer())
                // How far above the surface the sun is taken to stop reaching. The stock value,
                // for the stock light chain.
                .addRasterizer(new SunlightRasterizer(-20));
    }
}
