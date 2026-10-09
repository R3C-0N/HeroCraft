// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.distant;

import org.joml.Vector2i;
import org.joml.Vector3i;
import org.terasology.biomesAPI.Biome;
import org.terasology.core.world.generator.facets.BiomeFacet;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.facets.CaveFacet;
import org.terasology.herocubeworlds.generator.rasterizers.CubeCell;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.facets.DensityFacet;
import org.terasology.engine.world.generation.facets.SeaLevelFacet;
import org.terasology.engine.world.generation.facets.SurfacesFacet;

import java.util.Arrays;

/**
 * Summarises a node of the quadtree straight from the generator, without building a chunk.
 * <p>
 * The facets are asked for at the node's own scale — one cell per column, and as many blocks per
 * cell vertically as horizontally — and every cell goes through {@link CubeCell}, the rasterizer's
 * own decision. So a column summarised here is the column the world would have, at the resolution
 * asked for, and a node nobody has ever walked to is as full as one next door.
 * <p>
 * Nothing here needs the engine: the chain is a {@link World}, which a test builds with a
 * {@code WorldBuilder} and no block manager, and the materials are named by whoever calls. That is
 * what lets a census measure the distance without launching the game.
 */
public final class ColumnSummarizer {

    /** Names a material, so that runs can be told apart. The engine names them by block. */
    @FunctionalInterface
    public interface MaterialNamer {
        int id(CubeCell cell, Biome biome, Vector3i worldPos, int seaLevel, float density);

        /** Only asked of materials a player put there; the generator's cells know their own kind. */
        default boolean isLiquid(int material) {
            return false;
        }

        /**
         * The number of a material as it looks in a biome, for those named without one — a player's
         * block, a tree's leaves — since grass and leaves take their colour from the biome.
         */
        default int tinted(int material, Biome biome) {
            return material;
        }
    }

    /** Light of a cell open to the sky, on the engine's scale of fifteen. */
    public static final int FULL_LIGHT = 15;

    private final World world;
    private final CubeGeometry geometry;
    private final MaterialNamer namer;
    private final DistantEdits edits;
    private volatile DistantTrees trees;

    public ColumnSummarizer(World world, CubeGeometry geometry, MaterialNamer namer) {
        this(world, geometry, namer, null);
    }

    /** @param edits what players changed, read before the generator wherever it covers a cell */
    public ColumnSummarizer(World world, CubeGeometry geometry, MaterialNamer namer, DistantEdits edits) {
        this.world = world;
        this.geometry = geometry;
        this.namer = namer;
        this.edits = edits;
    }

    /** Grows the real trees on the fine levels, over the ground the generator makes. */
    void setTrees(DistantTrees newTrees) {
        this.trees = newTrees;
    }

    public ColumnSection summarize(SectionKey key) {
        int size = SectionKey.columns(geometry);
        int scale = key.stride();
        int[] origin = key.netOrigin(geometry);
        int bottom = Math.floorDiv(geometry.floorY(), scale);
        int top = Math.floorDiv(geometry.ceilingY(), scale);
        BlockRegion cells = new BlockRegion(origin[0] / scale, bottom, origin[1] / scale)
                .setSize(size, top - bottom + 1, size);
        Region region = world.getWorldData(cells, scale);

        DensityFacet density = region.getFacet(DensityFacet.class);
        SurfacesFacet surfaces = region.getFacet(SurfacesFacet.class);
        BiomeFacet biomes = region.getFacet(BiomeFacet.class);
        CaveFacet caves = region.getFacet(CaveFacet.class);
        int seaLevel = region.getFacet(SeaLevelFacet.class).getSeaLevel();
        DistantTrees.Cells treeCells = treesOf(cells, size, scale, surfaces, seaLevel);

        int[] offsets = new int[size * size + 1];
        long[] segments = new long[size * size * 4];
        int count = 0;
        Vector3i cell = new Vector3i();
        Vector3i worldPos = new Vector3i();
        Vector2i column = new Vector2i();
        for (int v = 0; v < size; v++) {
            for (int u = 0; u < size; u++) {
                offsets[u + v * size] = count;
                int cx = cells.minX() + u;
                int cz = cells.minZ() + v;
                Biome biome = biomes.getWorld(column.set(cx, cz));
                boolean edited = edits != null && edits.touches(cx * scale, cz * scale);
                // Past the fine levels, the trees are the biome's canopy, standing on the top surface.
                DistantTrees.Crown crown = null;
                int crownBase = 0;
                if (treeCells == null && trees != null) {
                    DistantTrees.Canopy canopy = trees.canopy(biome);
                    crown = canopy == null ? null : canopy.at(cx * scale, cz * scale);
                    int surfaceCell = crown == null ? Integer.MIN_VALUE : topSurface(surfaces, cx, cz, top, bottom, cell);
                    // The highest block of that cell is the ground; trees stand on land at or above the sea.
                    int ground = surfaceCell == Integer.MIN_VALUE ? Integer.MIN_VALUE : surfaceCell * scale + scale - 1;
                    if (ground < seaLevel || crown.leaf < 0) {
                        crown = null;
                    } else {
                        crownBase = ground + 1;
                    }
                }

                // Top down, so that the first run is the one the sky sees.
                int runTop = -1;
                int runBottom = -1;
                int runMaterial = -1;
                boolean runLiquid = false;
                CubeCell runKind = CubeCell.AIR;
                boolean skyOpen = true;
                for (int y = top; y >= bottom; y--) {
                    cell.set(cx, y, cz);
                    float d = density.getWorld(cell);
                    int basePosY = y * scale;
                    CubeCell kind = CubeCell.classify(geometry, d, surfaces.getWorld(cell), caves.getWorld(cell),
                            basePosY, seaLevel, scale, biome);
                    int material = -1;
                    boolean liquid;
                    int edit = edited ? edits.cell(cx * scale, basePosY, cz * scale) : DistantEdits.NO_EDIT;
                    if (edit != DistantEdits.NO_EDIT) {
                        // What a player left there, in place of what the generator would have put.
                        material = namer.tinted(edit, biome);
                        liquid = edit >= 0 && namer.isLiquid(edit);
                        kind = edit < 0 ? CubeCell.AIR : liquid ? CubeCell.WATER : CubeCell.SURFACE;
                    } else {
                        if (kind.isFilled()) {
                            worldPos.set(cx * scale, basePosY, cz * scale);
                            material = namer.id(kind, biome, worldPos, seaLevel, d);
                        }
                        liquid = kind.isLiquid();
                        // A tree is set over whatever the ground left there, as the rasterizer sets it.
                        int tree = treeCells == null ? -1 : treeCells.material(u, v, y);
                        if (crown != null && kind == CubeCell.AIR && basePosY >= crownBase + crown.bottom
                                && basePosY <= crownBase + crown.top) {
                            tree = crown.leaf;
                        }
                        if (tree >= 0) {
                            material = namer.tinted(tree, biome);
                            liquid = false;
                            kind = CubeCell.SURFACE;
                        }
                    }
                    if (material == runMaterial && liquid == runLiquid && material >= 0) {
                        runBottom = basePosY;
                        continue;
                    }
                    if (runMaterial >= 0) {
                        segments = grow(segments, count);
                        segments[count++] = seal(runBottom, runTop, runMaterial, runKind, skyOpen);
                        skyOpen &= runLiquid;
                    }
                    runMaterial = material;
                    runLiquid = liquid;
                    runKind = kind;
                    runTop = basePosY + scale - 1;
                    runBottom = basePosY;
                }
                if (runMaterial >= 0) {
                    segments = grow(segments, count);
                    segments[count++] = seal(runBottom, runTop, runMaterial, runKind, skyOpen);
                }
            }
        }
        offsets[size * size] = count;
        return new ColumnSection(key, size, geometry.floorY(), offsets, Arrays.copyOf(segments, count));
    }

    /**
     * The trees over a node of a fine level, grown from the heights its surfaces span: trees stand on
     * ground at or above the sea.
     */
    private DistantTrees.Cells treesOf(BlockRegion cells, int size, int scale, SurfacesFacet surfaces, int seaLevel) {
        DistantTrees current = trees;
        if (current == null || scale > DistantTrees.MAX_STRIDE) {
            return null;
        }
        int lowest = Integer.MAX_VALUE;
        int highest = Integer.MIN_VALUE;
        Vector3i cell = new Vector3i();
        for (int v = 0; v < size; v++) {
            for (int u = 0; u < size; u++) {
                for (int y = cells.maxY(); y >= cells.minY(); y--) {
                    if (surfaces.getWorld(cell.set(cells.minX() + u, y, cells.minZ() + v))) {
                        lowest = Math.min(lowest, y * scale);
                        highest = Math.max(highest, y * scale);
                        break;
                    }
                }
            }
        }
        // A cell stands for scale blocks: the ground is anywhere in it.
        return current.plant(cells.minX() * scale, cells.minZ() * scale, size, scale,
                Math.max(lowest, seaLevel), highest + scale - 1);
    }

    /** The cell of the highest surface of a column, or the floor if it has none. */
    private static int topSurface(SurfacesFacet surfaces, int cx, int cz, int top, int bottom, Vector3i cell) {
        for (int y = top; y >= bottom; y--) {
            if (surfaces.getWorld(cell.set(cx, y, cz))) {
                return y;
            }
        }
        return Integer.MIN_VALUE;
    }

    /**
     * The light a run is baked with. The sky reaches the first opaque run from the top and whatever
     * liquid lies above it, and nothing below: an overhang is dark underneath, which is the price
     * of baking. The only block light the generator makes is the lava's. The sky is kept for whoever
     * reads a column on its own; the mesher lights each vertex again from the runs around it, as the
     * engine lights a vertex from the blocks around it, which dims the bed of the sea with its depth.
     */
    private long seal(int runBottom, int runTop, int material, CubeCell kind, boolean skyOpen) {
        int floor = geometry.floorY();
        int blockLight = kind == CubeCell.CAVE_LAVA ? FULL_LIGHT : 0;
        return ColumnSection.pack(runBottom - floor, Math.min(runTop, geometry.ceilingY()) - floor, material,
                skyOpen ? FULL_LIGHT : 0, blockLight, kind.isLiquid());
    }

    private static long[] grow(long[] segments, int count) {
        return count < segments.length ? segments : Arrays.copyOf(segments, segments.length * 2);
    }
}
