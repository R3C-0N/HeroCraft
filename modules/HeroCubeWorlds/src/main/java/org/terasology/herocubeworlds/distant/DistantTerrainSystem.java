// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.distant;

import org.joml.Vector2fc;
import org.joml.Vector3fc;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.CubeWorldGenerator;
import org.terasology.herocubeworlds.generator.rasterizers.CubeSolidRasterizer;
import org.terasology.herocubeworlds.rendering.CubeSphereProjection;
import org.terasology.engine.config.Config;
import org.terasology.engine.context.Context;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.entitySystem.systems.UpdateSubscriberSystem;
import org.terasology.engine.logic.console.commandSystem.annotations.Command;
import org.terasology.engine.logic.console.commandSystem.annotations.CommandParam;
import org.terasology.engine.registry.In;
import org.terasology.engine.rendering.cameras.Camera;
import org.terasology.engine.rendering.sphere.SphereProjection;
import org.terasology.engine.rendering.world.DistantGeometry;
import org.terasology.engine.rendering.world.DistantReach;
import org.terasology.engine.rendering.world.RenderableWorldImpl;
import org.terasology.engine.rendering.world.WorldRenderer;
import org.terasology.engine.utilities.Assets;
import org.terasology.engine.world.OnChangedBlock;
import org.terasology.engine.world.WorldComponent;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockComponent;
import org.terasology.engine.world.block.BlockManager;
import org.terasology.engine.world.block.ColorProvider;
import org.terasology.engine.world.block.tiles.BlockTile;
import org.terasology.engine.world.block.tiles.WorldAtlas;
import org.terasology.engine.world.chunks.ChunkProvider;
import org.terasology.engine.world.chunks.Chunks;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generator.WorldGenerator;
import org.terasology.gestalt.assets.management.AssetManager;
import org.terasology.gestalt.entitysystem.event.ReceiveEvent;
import org.terasology.nui.Color;
import org.terasology.nui.Colorc;

import java.util.HashMap;
import java.util.Map;
import java.util.Set;
import java.util.function.Function;

/**
 * Keeps the distant terrain of a cube world, on the client that draws it.
 * <p>
 * Sized by the level-of-detail setting the engine already has — {@code chunkLods} in the
 * rendering config, the slider of the video settings — whose own provider never ran on this world.
 * It no longer switches anything: the setting has a floor of one
 * ({@link org.terasology.engine.config.RenderingConfig#MIN_CHUNK_LODS}), so the guard below is in
 * practice the world-generator test alone, and distant terrain cannot be turned off.
 * The distance is bounded by the horizon, taken over the sea for
 * the eye and for the highest ground the generator makes; past that nothing can be seen on a planet,
 * whatever the setting says.
 */
@RegisterSystem(RegisterMode.CLIENT)
public class DistantTerrainSystem extends BaseComponentSystem implements UpdateSubscriberSystem {

    private static final Logger logger = LoggerFactory.getLogger(DistantTerrainSystem.class);

    /** The finest level drawn past the band round the chunks: one column per two blocks. Level 0 is the band's. */
    private static final int FINEST_LEVEL = 1;
    /** How far inside the edge of the chunks the band of finest columns starts: a chunk may be missing there. */
    private static final float SHARP_INSIDE = Chunks.SIZE_X;
    /** The highest ground the generator makes, above the reference level: 62 + 26, with margin. */
    private static final double HIGHEST_GROUND = 100.0;
    /** Seconds between two recounts of the columns the game draws. */
    private static final float HOLE_PERIOD = 0.5f;
    /** Seconds between two batches of edits read back from the chunks. */
    private static final float EDIT_PERIOD = 2.0f;

    @In
    private Context context;
    @In
    private Config config;
    @In
    private WorldGenerator worldGenerator;
    @In
    private WorldRenderer worldRenderer;
    @In
    private ChunkProvider chunkProvider;
    @In
    private EntityManager entityManager;
    @In
    private BlockManager blockManager;
    @In
    private AssetManager assetManager;

    private CubeGeometry geometry;
    private DistantTerrain terrain;
    private DistantHole hole;
    private DistantPalette palette;
    /** The columns of chunks a drawn node of the distance covers, recounted with the hole. */
    private boolean[] distantColumns;
    private final DistantEdits edits = new DistantEdits();
    private float sinceHole;
    private float sinceEdits;
    private boolean unsupportedChunks;
    private int holeCentreX = Integer.MIN_VALUE;
    private int holeCentreZ = Integer.MIN_VALUE;
    private int holeRadius;

    @Override
    public void update(float delta) {
        if (config.getRendering().getChunkLods() <= 0 || !(worldGenerator instanceof CubeWorldGenerator)) {
            if (terrain != null) {
                shutdown();
            }
            return;
        }
        if (worldRenderer == null || terrain == null && !start()) {
            return;
        }
        Camera camera = worldRenderer.getActiveCamera();
        if (camera == null) {
            return;
        }
        Vector3fc eye = camera.getPosition();
        double reach = geometry.horizon(eye.y() - geometry.seaLevel()) + geometry.horizon(HIGHEST_GROUND);
        // Half the loaded box, from the camera's chunk: where the real ground ends.
        float frontier = (config.getRendering().getViewDistance().getChunkDistance().x() / 2 + 0.5f) * Chunks.SIZE_X;
        // Across it, single-block columns out to a node's width past the corners of the box, which are √2 further
        // than its sides: the coarsening starts away from the real ground all the way round.
        terrain.update(eye, reach, frontier - SHARP_INSIDE, frontier * Math.sqrt(2.0) + SectionKey.columns(geometry));
        state().setReach((float) reach, frontier);

        sinceEdits += delta;
        if (sinceEdits >= EDIT_PERIOD) {
            sinceEdits = 0;
            Set<Long> changed = edits.capture(chunkProvider, palette);
            if (!changed.isEmpty()) {
                terrain.invalidate(changed);
                save();
            }
        }

        // At once when the eye enters another chunk: a teleport would otherwise show the distance under
        // the player for up to a period.
        int centreX = Math.floorDiv((int) Math.floor(eye.x()), Chunks.SIZE_X);
        int centreZ = Math.floorDiv((int) Math.floor(eye.z()), Chunks.SIZE_Z);
        int radius = keptRadius(eye);
        boolean moved = centreX != holeCentreX || centreZ != holeCentreZ || radius != holeRadius;
        sinceHole += delta;
        if ((sinceHole >= HOLE_PERIOD || moved) && !unsupportedChunks) {
            sinceHole = 0;
            holeCentreX = centreX;
            holeCentreZ = centreZ;
            holeRadius = radius;
            try {
                if (distantColumns == null) {
                    distantColumns = new boolean[hole.side() * hole.side()];
                }
                terrain.coverage(distantColumns, hole.side());
                hole.mark(chunkProvider.getAllChunks(), centreX, centreZ, radius, distantColumns);
            } catch (UnsupportedOperationException e) {
                // A remote client cannot list its chunks: no hole, so the distance would be drawn
                // under the terrain. Better to say it once than to fight the depth buffer quietly.
                logger.warn("This client cannot list its chunks; the distant terrain has no hole.");
                unsupportedChunks = true;
            }
        }
    }

    /**
     * How many chunks round the eye the distance is kept out of, whatever is drawn: the ring the loading
     * screen waits for. None when the eye is too high above the sea for its chunks to be loaded at all —
     * a pilot that high sees the ground only through the distance, and a ring of sky under it would be
     * a hole onto nothing.
     */
    private int keptRadius(Vector3fc eye) {
        int halfHeight = config.getRendering().getViewDistance().getChunkDistance().y() / 2 * Chunks.SIZE_Y;
        return eye.y() - geometry.seaLevel() > halfHeight ? -1 : RenderableWorldImpl.PREGENERATED_RADIUS;
    }

    private boolean start() {
        geometry = context.get(CubeGeometry.class);
        World world = ((CubeWorldGenerator) worldGenerator).getWorld();
        SphereProjection sphere = context.get(SphereProjection.class);
        if (geometry == null || world == null || !(sphere instanceof CubeSphereProjection)) {
            // A remote client has the generator but never initialised it: there is no chain to ask.
            return false;
        }
        CubeSphereProjection projection = (CubeSphereProjection) sphere;
        CubeSolidRasterizer rasterizer = new CubeSolidRasterizer(geometry);
        rasterizer.initialize();
        palette = new DistantPalette(rasterizer);
        // A chunk tints grass and leaves by the world's colour provider, BiomesAPI's when it is there,
        // which reads these two tables; the distance reads the same ones.
        if (context.get(ColorProvider.class) != null) {
            palette.setBiomeColours(Assets.getTexture("engine:grasscolor").get().getData(),
                    Assets.getTexture("engine:foliagecolor").get().getData());
        }
        WorldAtlas atlas = context.get(WorldAtlas.class);
        if (atlas != null) {
            palette.setTileColours(tileColours(atlas));
        }
        DistantEditsComponent saved = worldEntity().getComponent(DistantEditsComponent.class);
        if (saved != null) {
            edits.loadFrom(saved, name -> {
                Block block = blockManager.getBlock(name);
                return block == null ? -1 : palette.idOf(block);
            });
            logger.info("Distant terrain: {} changed chunk(s) read back from the save", edits.capturedChunks());
        }
        ColumnSummarizer summarizer = new ColumnSummarizer(world, geometry, palette, edits);
        summarizer.setTrees(new DistantTrees(world, blockManager, palette));
        int threads = Math.max(1, Math.min(2, Runtime.getRuntime().availableProcessors() / 4));
        terrain = new DistantTerrain(geometry, summarizer, new DistantMesher(palette), FINEST_LEVEL, threads);
        hole = new DistantHole(geometry);
        state().set(terrain, new DistantDrawer(terrain, hole, projection));
        logger.info("Distant terrain on, {} node(s) built at a time", threads);
        return true;
    }

    /** The mean colour of every tile, by its place in the atlas: what a plant and its ground read as from afar. */
    private Function<Vector2fc, Colorc> tileColours(WorldAtlas atlas) {
        int size = atlas.getAtlasSize();
        Map<Long, Colorc> means = new HashMap<>();
        for (BlockTile tile : assetManager.getLoadedAssets(BlockTile.class)) {
            Vector2fc at = atlas.getTexCoords(tile, false);
            float[] mean = tile.getMeanColour();
            means.put(atlasKey(at, size), new Color(mean[0], mean[1], mean[2], 1.0f));
        }
        return at -> at == null ? null : means.get(atlasKey(at, size));
    }

    private static long atlasKey(Vector2fc at, int size) {
        return ((long) Math.round(at.x() * size) << 32) | (Math.round(at.y() * size) & 0xFFFFFFFFL);
    }

    /**
     * Every changed block, whoever changed it — a player, a liquid flowing, a mob digging. Noted
     * even while the distance is off, so that switching it on shows what was built before.
     */
    @ReceiveEvent(components = BlockComponent.class)
    public void onChangedBlock(OnChangedBlock event, EntityRef entity) {
        edits.note(event.getBlockPosition());
    }

    /** The captures go with the world, as a component of its entity: the engine saves it. */
    private void save() {
        EntityRef world = worldEntity();
        if (!world.exists()) {
            return;
        }
        DistantEditsComponent component = new DistantEditsComponent();
        edits.saveInto(component, id -> palette.block(id).getURI().toString());
        world.addOrSaveComponent(component);
    }

    private EntityRef worldEntity() {
        for (EntityRef entity : entityManager.getEntitiesWith(WorldComponent.class)) {
            return entity;
        }
        return EntityRef.NULL;
    }

    private DistantState state() {
        DistantState state = context.get(DistantState.class);
        if (state == null) {
            state = new DistantState();
            context.put(DistantState.class, state);
            context.put(DistantReach.class, state);
            context.put(DistantGeometry.class, state);
        }
        return state;
    }

    /** Shows or hides the distance, the chunks staying drawn: the only way to tell which of the two draws something. */
    @Command(shortDescription = "Shows or hides the distant terrain, for debugging",
            helpText = "With no argument, flips it. The chunks stay drawn, and nothing is rebuilt.")
    public String distantShow(@CommandParam(value = "shown", required = false) Boolean shown) {
        DistantState state = state();
        state.setHidden(shown == null ? !state.isHidden() : !shown);
        return state.isHidden() ? "distant terrain hidden" : "distant terrain shown";
    }

    /** One line for the inspection port and the bench. */
    @Command(shortDescription = "Reports the distant terrain: nodes, cost, memory, what was drawn",
            helpText = "One line: nodes built and shown, nodes waiting, mean build time, what the "
                    + "columns weigh, how many columns of chunks the game draws itself, and what the "
                    + "distant pass drew at the last frame.")
    public String distantStats() {
        if (terrain == null) {
            return "distant off (chunkLods=" + config.getRendering().getChunkLods() + ")";
        }
        DistantDrawer drawer = state().drawer();
        return terrain.stats() + " hole_columns=" + hole.drawnColumns() + " kept_columns=" + hole.keptColumns()
                + " edited_chunks=" + edits.capturedChunks()
                + " " + (drawer == null ? "pass=absent" : drawer.stats());
    }

    @Override
    public void shutdown() {
        if (terrain != null) {
            terrain.dispose();
            hole.dispose();
            state().set(null, null);
            terrain = null;
            hole = null;
        }
    }
}
