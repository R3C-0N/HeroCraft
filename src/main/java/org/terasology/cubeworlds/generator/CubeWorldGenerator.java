// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.generator;

import org.joml.Vector2i;
import org.joml.Vector2ic;
import org.joml.Vector3fc;
import org.terasology.core.world.generator.facetProviders.DefaultFloraProvider;
import org.terasology.core.world.generator.facetProviders.DefaultTreeProvider;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.core.world.generator.facetProviders.SurfaceToDensityProvider;
import org.terasology.core.world.generator.rasterizers.FloraRasterizer;
import org.terasology.core.world.generator.rasterizers.SunlightRasterizer;
import org.terasology.core.world.generator.rasterizers.TreeRasterizer;
import org.terasology.cubeworlds.generator.facetProviders.CubeBiomeProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeCaveProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeOreProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeRegionProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.cubeworlds.generator.rasterizers.CubeOreRasterizer;
import org.terasology.cubeworlds.generator.rasterizers.CubeSolidRasterizer;
import org.terasology.engine.context.Context;
import org.terasology.engine.core.SimpleUri;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.registry.In;
import org.terasology.engine.rendering.sphere.SphereProjection;
import org.terasology.engine.rendering.sphere.SphereProjectionProvider;
import org.terasology.engine.rendering.world.ChunkVisibilityPolicy;
import org.terasology.cubeworlds.rendering.CubeSphereProjection;
import org.terasology.cubeworlds.rendering.SphericalChunkVisibilityPolicy;
import org.terasology.engine.world.generation.BaseFacetedWorldGenerator;
import org.terasology.engine.world.generation.WorldBuilder;
import org.terasology.engine.world.generator.PreviewCentered;
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
public class CubeWorldGenerator extends BaseFacetedWorldGenerator
        implements SphereProjectionProvider, PreviewCentered {

    @In
    private WorldGeneratorPluginLibrary worldGeneratorPluginLibrary;

    @In
    private Context context;

    /**
     * The equatorial band: as far as one can get from the seven seams that face a dead slot, and
     * therefore from the void one would otherwise be looking at.
     */
    private static final int SPAWN_FACE = 1;

    private final CubeGeometry geometry = CubeGeometry.createDefault();
    private final CubeLandSpawner spawner;

    public CubeWorldGenerator(SimpleUri uri) {
        super(uri);
        // Not a FixedSpawner on the middle of the face: whether that point is land or ocean is a
        // coin toss on a world whose continents are a thousand blocks across, and on the first seed
        // ever played it came up ocean. CubeLandSpawner says why the stock one cannot be widened.
        spawner = new CubeLandSpawner(geometry, SPAWN_FACE);
    }

    public CubeGeometry getGeometry() {
        return geometry;
    }

    private CubeSphereProjection projection;
    private SphericalChunkVisibilityPolicy visibilityPolicy;

    @Override
    public void initialize() {
        // Published before the render graph is built, so generation and rendering read one and the
        // same radius from one and the same object.
        if (context != null) {
            context.put(CubeGeometry.class, geometry);
        }
        super.initialize();
    }

    /**
     * The projection and the culling policy come as a pair: drawing the world bent while culling it
     * flat would leave the renderer half convinced, and the seam between the two beliefs shows up as
     * chunks winking out at the horizon.
     */
    @Override
    public synchronized SphereProjection getSphereProjection() {
        if (projection == null) {
            projection = new CubeSphereProjection(geometry, context);
        }
        return projection;
    }

    @Override
    public synchronized ChunkVisibilityPolicy getChunkVisibilityPolicy() {
        if (visibilityPolicy == null) {
            visibilityPolicy = new SphericalChunkVisibilityPolicy(
                    (CubeSphereProjection) getSphereProjection());
        }
        return visibilityPolicy;
    }

    @Override
    public Vector3fc getSpawnPosition(EntityRef entity) {
        return spawner.getSpawnPosition(getWorld(), entity);
    }

    /**
     * The middle of the spawn face, and deliberately not the spawn point.
     * <p>
     * The net runs from the origin outwards in one direction only, so a preview framed on (0, 0)
     * sits entirely inside the dead slot at the corner — an empty square at every zoom the slider
     * can reach, whatever the seed. This is a constant of the layout and costs nothing, where
     * {@link #getSpawnPosition} now searches for land.
     */
    @Override
    public Vector2ic getPreviewCentre() {
        int[] centre = geometry.faceCentre(SPAWN_FACE);
        return new Vector2i(centre[0], centre[1]);
    }

    @Override
    protected WorldBuilder createWorld() {
        return new WorldBuilder(worldGeneratorPluginLibrary)
                .setSeaLevel(geometry.seaLevel())
                .addProvider(new SeaLevelProvider(geometry.seaLevel()))
                .addProvider(new CubeSurfaceProvider(geometry))
                .addProvider(new CubeClimateProvider(geometry))
                .addProvider(new CubeRegionProvider(geometry))
                .addProvider(new CubeBiomeProvider())
                .addProvider(new SurfaceToDensityProvider())
                // After the density and the surfaces are produced, and before the flora reads
                // them: the caves take surfaces away where they have eaten the ground, and a tree
                // chosen before that would be left standing in mid-air over a cave mouth.
                .addProvider(new CubeCaveProvider(geometry))
                // After the caves, and the chain sees to it: requiring the density puts a provider
                // below every priority that updates it, so the rock this reads is already carved.
                .addProvider(new CubeOreProvider(geometry))
                .addProvider(new DefaultFloraProvider())
                .addProvider(new DefaultTreeProvider())
                .addRasterizer(new CubeSolidRasterizer(geometry))
                // Straight after the ground is laid: the ore replaces rock, so it has to see what
                // the solid rasterizer wrote, and it must not be there for the flora to stand on.
                .addRasterizer(new CubeOreRasterizer())
                .addPlugins()
                .addRasterizer(new FloraRasterizer())
                .addRasterizer(new TreeRasterizer())
                // How far above the surface the sun is taken to stop reaching. The stock value,
                // for the stock light chain.
                .addRasterizer(new SunlightRasterizer(-20));
    }
}
