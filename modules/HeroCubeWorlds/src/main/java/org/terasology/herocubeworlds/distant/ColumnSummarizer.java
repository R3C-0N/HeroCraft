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
import org.terasology.engine.world.generation.facets.ElevationFacet;
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

        /** The number of a ground with a plant on it, coloured as the two look together from afar. */
        default int planted(int ground, int plant) {
            return ground;
        }
    }

    /** Light of a cell open to the sky, on the engine's scale of fifteen. */
    public static final int FULL_LIGHT = 15;

    /**
     * How far under the ground the distance still holds what is there, in blocks: below the lowest
     * sky-facing ground of a column and its four neighbours. The overhang criterion of the census —
     * a cave mouth or a gallery under a cliff is seen from afar within that depth, and nothing
     * deeper ever is, since past the loaded chunks nobody looks down a shaft. Below it the column is
     * solid to the floor of the world, so that no wall is built against a gallery nobody sees: those
     * walls were three quarters of the distant triangles.
     */
    public static final int KEPT_DEPTH = 32;
    /**
     * How far under the lowest elevation of a node the generator is asked at all. Twice the kept
     * depth, because the ground the sky sees can lie under the elevation where a cave opens to the
     * surface, and the kept depth is counted from there.
     */
    static final int ASKED_DEPTH = 2 * KEPT_DEPTH;

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
        int floorCell = Math.floorDiv(geometry.floorY(), scale);
        int[] elevation = elevations(origin, size, scale);
        int bottom = floorCell;
        int top = Math.floorDiv(geometry.ceilingY(), scale);
        if (elevation != null) {
            int lowestElevation = Integer.MAX_VALUE;
            int highestElevation = Integer.MIN_VALUE;
            for (int e : elevation) {
                lowestElevation = Math.min(lowestElevation, e);
                highestElevation = Math.max(highestElevation, e);
            }
            bottom = Math.max(floorCell, Math.floorDiv(lowestElevation - ASKED_DEPTH, scale));
            // Nothing the generator makes stands higher than a tree over the ground or the sea; what
            // a player built might, so a node with a capture in it is asked to the ceiling.
            if (!edited(origin, size, scale)) {
                int highest = Math.max(highestElevation, geometry.seaLevel()) + DistantTrees.reachUp() + scale;
                top = Math.max(bottom, Math.min(top, Math.floorDiv(highest, scale)));
            }
        }
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
                int plant = treeCells == null ? -1 : treeCells.plant(u, v);
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
                        } else if (plant >= 0 && material >= 0 && !liquid && runMaterial < 0) {
                            // The first ground under the sky is what the plant of the column stands on.
                            material = namer.planted(material, namer.tinted(plant, biome));
                            plant = -1;
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
        return keepWhatIsSeen(key, size, offsets, segments, count, elevation);
    }

    /** Whether a player changed anything under the node. */
    private boolean edited(int[] origin, int size, int scale) {
        if (edits == null) {
            return false;
        }
        for (int v = 0; v < size; v++) {
            for (int u = 0; u < size; u++) {
                if (edits.touches(origin[0] + u * scale, origin[1] + v * scale)) {
                    return true;
                }
            }
        }
        return false;
    }

    /**
     * The elevation of every column of a node, in blocks: the ground before the caves eat it and the
     * trees stand on it. Two dimensions only, so it costs the elevation's providers and none of the
     * volume's — the caves are what made a node cost most of a second. Null for a chain without an
     * elevation, which is then asked over its whole height.
     */
    private int[] elevations(int[] origin, int size, int scale) {
        BlockRegion columns = new BlockRegion(origin[0] / scale, 0, origin[1] / scale).setSize(size, 1, size);
        ElevationFacet elevation = world.getWorldData(columns, scale).getFacet(ElevationFacet.class);
        if (elevation == null) {
            return null;
        }
        int[] out = new int[size * size];
        for (int v = 0; v < size; v++) {
            for (int u = 0; u < size; u++) {
                out[u + v * size] = (int) Math.floor(elevation.getWorld(columns.minX() + u, columns.minZ() + v));
            }
        }
        return out;
    }

    /**
     * Drops the runs of every column that lie deeper than {@link #KEPT_DEPTH} under the lowest
     * sky-facing ground of the column and its neighbours in the node, and makes the column solid
     * from there to the floor of the world. Solid, and not cut: the mesher builds a wall wherever the
     * neighbour shows no ground, so a column that stopped short would be walled to the floor on four
     * sides. Between two columns that both reach the floor, there is nothing to draw.
     * <p>
     * The neighbours count because a cliff shows its face to the lower side: the high column keeps
     * what lies down to the foot of its neighbour, and a little more.
     */
    private ColumnSection keepWhatIsSeen(SectionKey key, int size, int[] offsets, long[] segments, int count,
                                         int[] elevation) {
        int columns = size * size;
        int[] ground = new int[columns];
        for (int c = 0; c < columns; c++) {
            ground[c] = Integer.MIN_VALUE;
            for (int i = offsets[c]; i < offsets[c + 1]; i++) {
                if (!ColumnSection.isLiquid(segments[i])) {
                    // Under the elevation where a cave opens to the sky, never over it for a crown:
                    // the leaves of a forest are not the ground a cliff shows down to.
                    int top = ColumnSection.relativeTop(segments[i]);
                    ground[c] = elevation == null ? top : Math.max(0, Math.min(top, elevation[c] - geometry.floorY()));
                    break;
                }
            }
        }
        long[] kept = new long[count + columns];
        int[] keptOffsets = new int[columns + 1];
        int n = 0;
        for (int v = 0; v < size; v++) {
            for (int u = 0; u < size; u++) {
                int c = u + v * size;
                keptOffsets[c] = n;
                int lowest = ground[c];
                if (lowest == Integer.MIN_VALUE) {
                    // Nothing opaque to stand on, as asked: keep the column as it is.
                    for (int i = offsets[c]; i < offsets[c + 1]; i++) {
                        kept[n++] = segments[i];
                    }
                    continue;
                }
                lowest = lowestNeighbour(ground, size, u, v, lowest);
                int cut = lowest - KEPT_DEPTH;
                int i = offsets[c];
                for (; i < offsets[c + 1] && ColumnSection.relativeTop(segments[i]) >= cut; i++) {
                    kept[n++] = segments[i];
                }
                long last = kept[n - 1];
                if (!ColumnSection.isLiquid(last)) {
                    kept[n - 1] = ColumnSection.downToFloor(last);
                    continue;
                }
                // A liquid straddles the cut, a lake in a gallery: ground under it, of the rock below.
                int under = ColumnSection.relativeBottom(last) - 1;
                if (under >= 0) {
                    int material = ColumnSection.material(deepestGround(segments, offsets[c], offsets[c + 1], i));
                    kept[n++] = ColumnSection.pack(0, under, material, 0, 0, false);
                }
            }
        }
        keptOffsets[columns] = n;
        return new ColumnSection(key, size, geometry.floorY(), keptOffsets, Arrays.copyOf(kept, n));
    }

    /** The lowest ground among a column and its four neighbours inside the node. */
    private static int lowestNeighbour(int[] ground, int size, int u, int v, int own) {
        int lowest = own;
        if (u > 0) {
            lowest = lowerGround(lowest, ground[u - 1 + v * size]);
        }
        if (u < size - 1) {
            lowest = lowerGround(lowest, ground[u + 1 + v * size]);
        }
        if (v > 0) {
            lowest = lowerGround(lowest, ground[u + (v - 1) * size]);
        }
        if (v < size - 1) {
            lowest = lowerGround(lowest, ground[u + (v + 1) * size]);
        }
        return lowest;
    }

    private static int lowerGround(int lowest, int other) {
        return other == Integer.MIN_VALUE ? lowest : Math.min(lowest, other);
    }

    /** The first opaque run from {@code from} down, or the last opaque run of the column above it. */
    private static long deepestGround(long[] segments, int start, int end, int from) {
        for (int i = from; i < end; i++) {
            if (!ColumnSection.isLiquid(segments[i])) {
                return segments[i];
            }
        }
        for (int i = from - 1; i >= start; i--) {
            if (!ColumnSection.isLiquid(segments[i])) {
                return segments[i];
            }
        }
        return segments[start];
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
        // The sea stops at the sea level, not at the top of the cell holding it. A cell of four blocks at level 2
        // counted 32 to 35 as water, and the distant sea stood three blocks above the sea of the chunks beside it
        // — seven at level 3. Where the two met, the distance was cut away over the chunks and the lower real
        // water did not cover the same pixels: a pale band of bare sea bed along the edge of the loaded box, and
        // straight down a cut seam, where the loaded box ends against the distance.
        if (kind == CubeCell.WATER || kind == CubeCell.SEA_STRADDLE) {
            runTop = Math.max(runBottom, Math.min(runTop, geometry.seaLevel()));
        }
        return ColumnSection.pack(runBottom - floor, Math.min(runTop, geometry.ceilingY()) - floor, material,
                skyOpen ? FULL_LIGHT : 0, blockLight, kind.isLiquid());
    }

    private static long[] grow(long[] segments, int count) {
        return count < segments.length ? segments : Arrays.copyOf(segments, segments.length * 2);
    }
}
