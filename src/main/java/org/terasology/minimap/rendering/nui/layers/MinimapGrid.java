// Copyright 2020 The Terasology Foundation
// SPDX-License-Identifier: Apache-2.0
package org.terasology.minimap.rendering.nui.layers;

import com.google.common.base.Preconditions;
import com.google.common.cache.CacheBuilder;
import com.google.common.cache.CacheLoader;
import com.google.common.cache.LoadingCache;
import com.google.common.collect.LinkedHashMultimap;
import com.google.common.collect.Multimap;
import org.joml.Quaternionf;
import org.joml.Vector2f;
import org.joml.Vector2fc;
import org.joml.Vector2i;
import org.joml.Vector2ic;
import org.joml.Vector3f;
import org.joml.Vector3i;
import org.joml.Vector3ic;
import org.terasology.engine.registry.CoreRegistry;
import org.terasology.engine.world.block.tiles.WorldAtlas;
import org.terasology.gestalt.assets.ResourceUrn;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.logic.location.LocationComponent;
import org.terasology.engine.rendering.assets.texture.BasicTextureRegion;
import org.terasology.engine.rendering.assets.texture.Texture;
import org.terasology.engine.rendering.assets.texture.TextureRegion;
import org.terasology.engine.rendering.nui.CanvasUtility;
import org.terasology.engine.utilities.Assets;
import org.terasology.engine.world.WorldProvider;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockAppearance;
import org.terasology.engine.world.block.BlockPart;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.chunks.Chunks;
import org.terasology.joml.geom.Rectanglef;
import org.terasology.joml.geom.Rectanglei;
import org.terasology.math.TeraMath;
import org.terasology.minimap.MinimapIconComponent;
import org.terasology.minimap.overlays.MinimapOverlay;
import org.terasology.nui.Canvas;
import org.terasology.nui.Color;
import org.terasology.nui.CoreWidget;
import org.terasology.nui.ScaleMode;
import org.terasology.nui.SubRegion;
import org.terasology.nui.databinding.Binding;
import org.terasology.nui.databinding.DefaultBinding;
import org.terasology.nui.databinding.ReadOnlyBinding;
import org.terasology.nui.util.RectUtility;

import java.util.Collection;
import java.util.Comparator;
import java.util.HashSet;
import java.util.Optional;
import java.util.PriorityQueue;
import java.util.Set;
import java.util.function.IntFunction;

/**
 * This is the actual minimap. All rendering-related code is located here.
 */
public class MinimapGrid extends CoreWidget {
    /**
     * The size of a cell (i.e. represents one block)
     */
    private static final Vector2ic CELL_SIZE = new Vector2i(4, 4);
    private static final Vector2ic BUFFER_SIZE = new Vector2i(
            CELL_SIZE.x() * Chunks.SIZE_X, CELL_SIZE.y() * Chunks.SIZE_Z);

    /**
     * The delta scale factor
     */
    private static final float ZOOM_DELTA = 0.25f;

    /**
     * Fond de la cuvette, `--surface-sunken` : pose sous chaque case, et seul
     * visible la ou le monde n'est pas encore charge.
     */
    private static final Color SUNKEN = new Color(0x20120AFF);
    /** `--sky-200` : eau d'un seul bloc. */
    private static final Color WATER_SHALLOW = new Color(0x48C8F5FF);
    /** `--sky-300` : eau de {@link #WATER_DEPTH_MAX} blocs et plus. */
    private static final Color WATER_DEEP = new Color(0x1F7FC0FF);
    private static final int WATER_DEPTH_MAX = 8;

    private Binding<EntityRef> targetEntityBinding = new DefaultBinding<>(EntityRef.NULL);
    private Binding<Integer> zoomFactorBinding = new DefaultBinding<>(0);

    private final Set<EntityRef> alivePlayers = new HashSet<EntityRef>();

    private Texture textureAtlas;

    private Multimap<Vector2ic, Vector3i> dirtyBlocks = LinkedHashMultimap.create();

    private WorldProvider worldProvider;

    private final Collection<MinimapOverlay> overlays =
            new PriorityQueue<>(Comparator.comparingInt(MinimapOverlay::getZOrder));

    private LoadingCache<Block, TextureRegion> cache =
            CacheBuilder.newBuilder().build(new CacheLoader<Block, TextureRegion>() {

                @Override
                public TextureRegion load(Block block) {
                    BlockAppearance primaryAppearance = block.getPrimaryAppearance();

                    BlockPart blockPart = BlockPart.TOP;

                    WorldAtlas worldAtlas = CoreRegistry.get(WorldAtlas.class);
                    float tileSize = worldAtlas.getRelativeTileSize();

                    Vector2fc textureAtlasPos = primaryAppearance.getTextureAtlasPos(blockPart);

                    return new BasicTextureRegion(textureAtlas, textureAtlasPos, new Vector2f(tileSize, tileSize));
                }

            });

    private IntFunction<Float> brightness;

    public MinimapGrid() {
        textureAtlas = Assets.getTexture("engine:terrain").get();
    }

    public void setHeightRange(int bottom, int top) {
        Preconditions.checkArgument(top > bottom);

        float minBright = 0.5f;
        float fac = (1 - minBright) / (top - bottom);
        brightness = y -> TeraMath.clamp(minBright + (y - bottom) * fac);
    }

    public void setWorldProvider(WorldProvider worldProvider) {
        this.worldProvider = worldProvider;
    }

    @Override
    public void update(float delta) {
        super.update(delta);
    }

    public void updateLocation(Vector3ic worldLocation) {
        Vector3i chunkPos = Chunks.toChunkPos(worldLocation, new Vector3i());
        Vector3i blockPos = Chunks.toRelative(worldLocation, new Vector3i());
        dirtyBlocks.put(new Vector2i(chunkPos.x(), chunkPos.z()), blockPos);
    }

    @Override
    public void onDraw(Canvas canvas) {

        EntityRef entity = getTargetEntity();
        LocationComponent locationComponent = entity.getComponent(LocationComponent.class);
        if (locationComponent == null) {
            return;
        }

        Vector3f centerPosition = locationComponent.getWorldPosition(new Vector3f());
        // See what we're walking on, not what's at knee level
        // the location (0/0) is at the center of the block 0/0
        // we therefore add an offset of 0.5 to the map to reflect this fact
        centerPosition.add(.5f, -1f, .5f);

        // define zoom factor
        int zoomLevel = getZoomFactor();
        float zoom = (float) Math.pow(2.0, zoomLevel * ZOOM_DELTA);
        int width = getPreferredContentSize().x();
        int height = getPreferredContentSize().y();
        float numberOfRows = height / (zoom * CELL_SIZE.y());
        float numberOfCols = width / (zoom * CELL_SIZE.x());

        int rowCenter = TeraMath.ceilToInt(numberOfRows * 0.5f);
        int colCenter = TeraMath.ceilToInt(numberOfCols * 0.5f);

        int centerX = TeraMath.floorToInt(centerPosition.x());
        int centerY = TeraMath.floorToInt(centerPosition.y());
        int centerZ = TeraMath.floorToInt(centerPosition.z());
        Vector3i minChunkPos = Chunks.toChunkPos(centerX - colCenter, 0, centerZ - rowCenter, new Vector3i());
        Vector3i maxChunkPos = Chunks.toChunkPos(centerX + colCenter, 0, centerZ + rowCenter, new Vector3i());

        int screenWidth = TeraMath.ceilToInt(BUFFER_SIZE.x() * zoom);
        int screenHeight = TeraMath.ceilToInt(BUFFER_SIZE.y() * zoom);

        Vector2i chunkPos = new Vector2i();
        Vector3i chunkDisc = new Vector3i(Chunks.SIZE_X, Chunks.SIZE_Y * 2, Chunks.SIZE_Z);
        float cellWidth = CELL_SIZE.x() * zoom;
        float cellHeight = CELL_SIZE.y() * zoom;
        for (int chunkZ = minChunkPos.z(); chunkZ <= maxChunkPos.z(); chunkZ++) {
            for (int chunkX = minChunkPos.x(); chunkX <= maxChunkPos.x(); chunkX++) {
                chunkPos.set(chunkX, chunkZ);
                ResourceUrn urn = new ResourceUrn("Minimap:gridcache" + chunkX + "x" + chunkZ);
                Optional<? extends TextureRegion> opt = Assets.get(urn, Texture.class);

                // create and render to FBO texture asset, if needed
                if (!opt.isPresent()) {
                    Vector3i worldPos = new Vector3i(chunkX * Chunks.SIZE_X, 0, chunkZ * Chunks.SIZE_Z);
                    BlockRegion region = new BlockRegion(worldPos).setSize(chunkDisc);
                    if (worldProvider.isRegionRelevant(region)) {
                        try (SubRegion ignored = CanvasUtility.subRegionFBO(canvas, urn, BUFFER_SIZE)) {
                            // use player's center Y pos to start searching for the surface layer
                            renderFullChunk(canvas, chunkX, chunkZ, centerY);
                        }
                        dirtyBlocks.removeAll(chunkPos);
                        opt = Assets.get(urn, Texture.class);
                    }
                }

                // update dirty blocks in cache texture
                Collection<Vector3i> chunkBlocks = dirtyBlocks.get(chunkPos);
                if (!chunkBlocks.isEmpty()) {
                    try (SubRegion ignored = CanvasUtility.subRegionFBO(canvas, urn, BUFFER_SIZE)) {
                        for (Vector3i pos : chunkBlocks) {
                            renderDirtyBlock(canvas, chunkX, chunkZ, pos);
                        }
                    }
                    dirtyBlocks.removeAll(chunkPos);
                }

                // render the actual chunk FBO texture
                if (opt.isPresent()) {
                    try (SubRegion ignored = canvas.subRegion(canvas.getRegion(), true)) {
                        float tileX = numberOfCols * 0.5f + chunkX * Chunks.SIZE_X - centerPosition.x();
                        float tileZ = numberOfRows * 0.5f + chunkZ * Chunks.SIZE_Z - centerPosition.z();

                        int offX = TeraMath.floorToInt(tileX * cellWidth);
                        int offZ = TeraMath.floorToInt(tileZ * cellHeight);

                        Rectanglei screenRegion = RectUtility.createFromMinAndSize(offX, offZ, screenWidth, screenHeight);
                        canvas.drawTextureRaw(opt.get(), screenRegion, ScaleMode.SCALE_FIT, 0f, 1f, 1f, -1f);
                    }
                }

            }
        }

        // render overlays
        Rectanglef worldRect = RectUtility.createFromCenterAndSize(
                centerPosition.x(), centerPosition.z(),
                (width / cellWidth), (height / cellHeight));

        try (SubRegion ignored = canvas.subRegion(canvas.getRegion(), true)) {
            for (MinimapOverlay overlay : overlays) {
                overlay.render(canvas, new Rectanglei((int) worldRect.minX, (int) worldRect.minY, (int) worldRect.maxX, (int) worldRect.maxY));
            }
        }

        drawPlayerArrows(canvas, zoom, centerX, centerZ);
    }

    private void renderFullChunk(Canvas canvas, int chunkX, int chunkZ, int startY) {
        for (int row = 0; row < Chunks.SIZE_Z; row++) {
            for (int column = 0; column < Chunks.SIZE_X; column++) {
                int x = column * CELL_SIZE.x();
                int y = row * CELL_SIZE.y();
                Rectanglei rect = RectUtility.createFromMinAndSize(x, y, CELL_SIZE.x(), CELL_SIZE.y());

                int blockX = chunkX * Chunks.SIZE_X + column;
                int blockZ = chunkZ * Chunks.SIZE_Z + row;
                Vector3i relLocation = new Vector3i(blockX, startY, blockZ);
                renderCell(canvas, rect, relLocation); // the y component of relLocation is modified!
            }
        }
    }

    private void renderDirtyBlock(Canvas canvas, int chunkX, int chunkZ, Vector3i pos) {
        int column = pos.x();
        int row = pos.z();
        int startY = pos.y();
        int x = column * CELL_SIZE.x();
        int y = row * CELL_SIZE.y();
        Rectanglei rect = RectUtility.createFromMinAndSize(x, y, CELL_SIZE.x(), CELL_SIZE.y());

        int blockX = chunkX * Chunks.SIZE_X + column;
        int blockZ = chunkZ * Chunks.SIZE_Z + row;
        Vector3i relLocation = new Vector3i(blockX, startY, blockZ);
        renderCell(canvas, rect, relLocation); // the y component of relLocation is modified!
    }

    private void drawPlayerArrows(Canvas canvas, float zoom, int centerX, int centerZ) {

        int width = getPreferredContentSize().x();
        int height = getPreferredContentSize().y();

        for (EntityRef alivePlayer : alivePlayers) {
            LocationComponent playerLocationComponent = alivePlayer.getComponent(LocationComponent.class);
            MinimapIconComponent minimapIconComponent = alivePlayer.getComponent(MinimapIconComponent.class);
            if (playerLocationComponent != null && minimapIconComponent != null) {

                Assets.getTexture(minimapIconComponent.iconUrn).ifPresent(icon -> {
                    //TODO UITexture should be used here, but it doesn't work
                    Assets.getMaterial("engine:UILitMesh").ifPresent(material -> {
                        material.setTexture("texture", icon);
                        Assets.getMesh("engine:UIBillboard").ifPresent(mesh -> {
                            // Drawing textures with rotation is not yet supported, see #1926
                            // We therefore use a workaround based on mesh drawing
                            // The width of the screenArea is doubled to avoid clipping issues when the texture is rotated
                            int arrowWidth = icon.getWidth() * 2;
                            int arrowHeight = icon.getHeight() * 2;
                            int arrowX = (width - arrowWidth) / 2;
                            int arrowY = (height - arrowHeight) / 2;
                            //canvas.drawTexture(arrowhead, arrowX, arrowY, rotation);

                            Vector3f playerPosition = playerLocationComponent.getWorldPosition(new Vector3f());
                            int xOffset = TeraMath.floorToInt((playerPosition.x() - centerX) * CELL_SIZE.x() * zoom);
                            int zOffset = TeraMath.floorToInt((playerPosition.z() - centerZ) * CELL_SIZE.y() * zoom);

                            if (isInBounds(width, height, xOffset, zOffset)) {
                                // The scaling seems to be completely wrong - 0.8f looks ok
                                Quaternionf q = playerLocationComponent.getWorldRotation(new Quaternionf());
                                // convert to Euler yaw angle
                                // TODO: move into quaternion
                                float rotation = -(float) Math.atan2(2.0 * (q.y * q.w + q.x * q.z), 1.0 - 2.0 * (q.y * q.y - q.z * q.z));
                                Rectanglei screenArea = RectUtility.createFromMinAndSize(arrowX + xOffset, arrowY + zOffset, arrowWidth, arrowHeight);
                                CanvasUtility.drawMesh(canvas, mesh, icon, screenArea, new Quaternionf().rotationYXZ(0, 0, rotation), new Vector3f(), 0.8f);
                            }
                        });
                    });
                });
            }
        }
    }

    /**
     * Determine whether a coordinate is still in bounds given by `width` x `height` after applying an offset.
     *
     * @return true if the offset coordinates are within the bounds determined by `width`x`height`.
     */
    private boolean isInBounds(int width, int height, int xOffset, int zOffset) {
        return xOffset <= (width / 2) && xOffset >= -(width / 2)
                && zOffset <= (height / 2) && zOffset >= -(height / 2);
    }

    @Override
    public Vector2i getPreferredContentSize(Canvas canvas, Vector2i sizeHint) {
        // la marge du skin est ajoutee par le canvas ; y ajouter la bordure du
        // fond decalerait la carte dans son cadre
        return getPreferredContentSize();
    }

    /**
     * La cuvette de l'artboard 2g : 200 px de haut dans un panneau de 320 px,
     * dont le cadre slim prend 30 px de chaque cote.
     */
    public Vector2i getPreferredContentSize() {
        return new Vector2i(260, 200);
    }

    public void bindTargetEntity(Binding<EntityRef> binding) {
        targetEntityBinding = binding;
    }

    public EntityRef getTargetEntity() {
        return targetEntityBinding.get();
    }

    public void setTargetEntity(EntityRef val) {
        targetEntityBinding.set(val);
    }

    public int getZoomFactor() {
        return zoomFactorBinding.get();
    }

    public void setZoomFactor(int val) {
        zoomFactorBinding.set(val);
    }

    public void bindZoomFactor(ReadOnlyBinding<Integer> offsetBinding) {
        zoomFactorBinding = offsetBinding;
    }

    private void renderCell(Canvas canvas, Rectanglei rect, Vector3i pos) {
        // Fond opaque d'abord : le FBO part transparent, et une tuile translucide
        // y laisserait voir le monde derriere le HUD.
        canvas.drawFilledRectangle(rect, SUNKEN);

        Block block = worldProvider.getBlock(pos);
        if (isIgnored(block)) {
            do {
                pos.y -= 1;
                if (!worldProvider.isBlockRelevant(pos)) {
                    return;
                }
                block = worldProvider.getBlock(pos);
            } while (isIgnored(block));
        } else {
            do {
                pos.y += 1;
                if (!worldProvider.isBlockRelevant(pos)) {
                    return;
                }
                block = worldProvider.getBlock(pos);
            } while (!isIgnored(block));
            pos.y -= 1;
        }

        Block surface = worldProvider.getBlock(pos);
        if (surface.isWater()) {
            // La tuile d'eau est un bleu-gris pale a 80 % d'alpha, pense pour
            // etre teinte par le shader : sur la carte, elle se lisait comme de
            // la pierre. On pose la couleur du design, foncee par la profondeur.
            canvas.drawFilledRectangle(rect, waterColor(pos));
            return;
        }

        float g = brightness.apply(pos.y());
        Color color = new Color(g, g, g);

        TextureRegion reg = cache.getUnchecked(surface);
        canvas.drawTexture(reg, rect, color);

        pos.y += 1;
        Block top = worldProvider.getBlock(pos);

        if (top.isDestructible()) {
            reg = cache.getUnchecked(top);
            canvas.drawTexture(reg, rect);
        }
    }

    /**
     * @param surface le bloc d'eau le plus haut de la colonne
     */
    private Color waterColor(Vector3ic surface) {
        Vector3i pos = new Vector3i(surface);
        int depth = 0;
        while (depth < WATER_DEPTH_MAX && worldProvider.isBlockRelevant(pos)
                && worldProvider.getBlock(pos).isWater()) {
            depth++;
            pos.y -= 1;
        }
        float t = Math.max(0, depth - 1) / (float) (WATER_DEPTH_MAX - 1);
        return new Color(
                WATER_SHALLOW.rf() + (WATER_DEEP.rf() - WATER_SHALLOW.rf()) * t,
                WATER_SHALLOW.gf() + (WATER_DEEP.gf() - WATER_SHALLOW.gf()) * t,
                WATER_SHALLOW.bf() + (WATER_DEEP.bf() - WATER_SHALLOW.bf()) * t);
    }

    private static boolean isIgnored(Block block) {
        return block.isPenetrable() && !block.isWater();
    }

    public void updateAlivePlayerList(Iterable<EntityRef> alivePlayersIterable) {
        for (EntityRef player : alivePlayersIterable) {
            alivePlayers.add(player);
        }
    }

    public void addAlivePlayer(EntityRef player) {
        alivePlayers.add(player);
    }

    public void removeAlivePlayer(EntityRef player) {
        alivePlayers.remove(player);
    }

    /**
     * @param overlay the overlay to add
     */
    public void addOverlay(MinimapOverlay overlay) {
        overlays.add(overlay);
    }

    /**
     * @param overlay the overlay to remove
     */
    public void removeOverlay(MinimapOverlay overlay) {
        overlays.remove(overlay);
    }

}
