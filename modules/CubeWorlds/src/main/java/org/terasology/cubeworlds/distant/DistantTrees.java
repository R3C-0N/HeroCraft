// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds.distant;

import org.joml.Vector3i;
import org.joml.Vector3ic;
import org.terasology.biomesAPI.Biome;
import org.terasology.core.world.CoreBiome;
import org.terasology.core.world.generator.facets.TreeFacet;
import org.terasology.core.world.generator.trees.TreeGenerator;
import org.terasology.core.world.generator.trees.Trees;
import org.terasology.engine.utilities.random.FastRandom;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockManager;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;

import java.util.ArrayList;
import java.util.EnumMap;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * The trees of a node, exactly: where the generator puts them, grown by their own generator.
 * <p>
 * The scaled chain the distance is summarised from has no trees — their provider places a tree on a
 * column by white noise and a flatness test on its four neighbours, which means nothing on a grid
 * of every other column. So the fine levels ask the chain at full scale for the tree facet over the
 * node, only over the heights where trees can stand, and grow every tree it lists into a
 * {@link TreeRecorder} with the seed the rasterizer gives it: the same tree, block for block. The
 * blocks are then read where the columns of the node read the ground, one block in {@code stride}
 * on every axis, so a crown in the distance is a crown sampled as the hill under it is.
 * <p>
 * Only for the fine levels: the full-scale surface under a coarse node costs more than the node,
 * and a tree there is a few pixels at most. The coarse levels get a {@link Canopy} instead: as much
 * of the ground under leaves as the biome's trees would cover, where a hash of the column says, with
 * crowns as wide and as high as the real generators grow them — measured on them, not guessed.
 */
final class DistantTrees {

    /** Up to this stride the trees are the real ones. */
    static final int MAX_STRIDE = 4;
    /** How far a tree reaches from its base, sideways, and above it: the tree generators' own bounds. */
    private static final int REACH_SIDEWAYS = 13;
    private static final int REACH_UP = 35;

    /** Trees measured per species to size its crown. */
    private static final int SAMPLES = 24;
    /** The tree provider's gate: its default density, 0.2, times its own 0.05. */
    private static final double TREE_GATE = 0.2 * 0.05;

    private final World world;
    private final BlockManager blockManager;
    private final DistantPalette palette;
    private volatile Map<CoreBiome, Canopy> canopies;

    DistantTrees(World world, BlockManager blockManager, DistantPalette palette) {
        this.world = world;
        this.blockManager = blockManager;
        this.palette = palette;
    }

    /**
     * The trees of each biome, as {@code DefaultTreeProvider} registers them — its table is private,
     * so its numbers are repeated here, and a change there has to be made here too. The cactus is left
     * out: a column of green one block wide is nothing at these distances.
     */
    private static Map<CoreBiome, Map<TreeGenerator, Float>> treesByBiome() {
        TreeGenerator oak = Trees.oakTree();
        TreeGenerator oakVariation = Trees.oakVariationTree();
        TreeGenerator red = Trees.redTree();
        TreeGenerator pine = Trees.pineTree();
        TreeGenerator birch = Trees.birchTree();
        Map<CoreBiome, Map<TreeGenerator, Float>> table = new EnumMap<>(CoreBiome.class);
        table.put(CoreBiome.MOUNTAINS, Map.of(oak, 0.04f, pine, 0.06f));
        table.put(CoreBiome.FOREST, Map.of(oak, 0.25f, oakVariation, 0.25f, red, 0.05f, pine, 0.10f, birch, 0.10f));
        table.put(CoreBiome.SNOW, Map.of(birch, 0.02f, pine, 0.10f));
        table.put(CoreBiome.PLAINS, Map.of(red, 0.01f, birch, 0.03f, oak, 0.01f));
        table.put(CoreBiome.TAIGA, Map.of(pine, 0.22f, birch, 0.03f));
        table.put(CoreBiome.SAVANNA, Map.of(oak, 0.02f));
        table.put(CoreBiome.SWAMP, Map.of(oakVariation, 0.08f));
        return table;
    }

    /** A kind of tree, as its generator grows it: its leaves, how many columns they cover, from what height to what. */
    static final class Crown {
        final int leaf;
        final double footprint;
        final int bottom;
        final int top;

        Crown(int leaf, double footprint, int bottom, int top) {
            this.leaf = leaf;
            this.footprint = footprint;
            this.bottom = bottom;
            this.top = top;
        }
    }

    /**
     * What the trees of a biome look like from far off: the share of the ground under leaves, and the
     * crowns that make it up, each weighed by how much ground it covers.
     */
    static final class Canopy {
        private final double cover;
        private final List<Crown> crowns = new ArrayList<>();
        private final List<Double> weights = new ArrayList<>();
        private double totalWeight;

        Canopy(double cover) {
            this.cover = cover;
        }

        double cover() {
            return cover;
        }

        void add(Crown crown, double weight) {
            crowns.add(crown);
            weights.add(weight);
            totalWeight += weight;
        }

        /** The crown standing on this column, or null for open ground. */
        Crown at(int x, int z) {
            if (crowns.isEmpty() || unit(hash(x, z, 0x51A7)) >= cover) {
                return null;
            }
            double pick = unit(hash(x, z, 0x0C4B)) * totalWeight;
            for (int i = 0; i < crowns.size(); i++) {
                pick -= weights.get(i);
                if (pick < 0) {
                    return crowns.get(i);
                }
            }
            return crowns.get(crowns.size() - 1);
        }

        private static long hash(int x, int z, long salt) {
            long h = x * 0x9E3779B97F4A7C15L ^ z * 0xC2B2AE3D27D4EB4FL ^ salt;
            h ^= h >>> 33;
            h *= 0xFF51AFD7ED558CCDL;
            h ^= h >>> 33;
            h *= 0xC4CEB9FE1A85EC53L;
            return h ^ (h >>> 33);
        }

        private static double unit(long h) {
            return (h >>> 11) * 0x1.0p-53;
        }
    }

    /**
     * The canopy of a biome, for the coarse levels. Poisson: with {@code λ} the ground a tree's crown
     * covers times the trees per column, a column is under leaves with probability {@code 1 - e^-λ},
     * overlaps counted once.
     *
     * @return null where the biome grows no trees
     */
    Canopy canopy(Biome biome) {
        Map<CoreBiome, Canopy> known = canopies;
        if (known == null) {
            known = measure();
            canopies = known;
        }
        return biome instanceof CoreBiome ? known.get(biome) : null;
    }

    private Map<CoreBiome, Canopy> measure() {
        Map<TreeGenerator, Crown> crowns = new HashMap<>();
        Map<CoreBiome, Canopy> result = new EnumMap<>(CoreBiome.class);
        for (Map.Entry<CoreBiome, Map<TreeGenerator, Float>> biome : treesByBiome().entrySet()) {
            double lambda = 0;
            for (Map.Entry<TreeGenerator, Float> tree : biome.getValue().entrySet()) {
                Crown crown = crowns.computeIfAbsent(tree.getKey(), this::grow);
                lambda += TREE_GATE * tree.getValue() * crown.footprint;
            }
            Canopy canopy = new Canopy(1.0 - Math.exp(-lambda));
            for (Map.Entry<TreeGenerator, Float> tree : biome.getValue().entrySet()) {
                Crown crown = crowns.get(tree.getKey());
                canopy.add(crown, tree.getValue() * crown.footprint);
            }
            result.put(biome.getKey(), canopy);
        }
        return result;
    }

    /** Grows a few trees of a kind and measures their crowns: the mean footprint, the median heights. */
    private Crown grow(TreeGenerator generator) {
        TreeRecorder recorder = new TreeRecorder();
        double footprint = 0;
        List<Integer> bottoms = new ArrayList<>();
        List<Integer> tops = new ArrayList<>();
        Block[] leaf = new Block[1];
        for (int seed = 0; seed < SAMPLES; seed++) {
            Set<Long> columns = new HashSet<>();
            int[] range = {Integer.MAX_VALUE, Integer.MIN_VALUE};
            recorder.into((dx, dy, dz, block) -> {
                if (!block.getURI().toString().contains("Leaf")) {
                    return;
                }
                leaf[0] = block;
                columns.add(((long) dx << 32) | (dz & 0xFFFFFFFFL));
                range[0] = Math.min(range[0], dy);
                range[1] = Math.max(range[1], dy);
            });
            generator.generate(blockManager, recorder, new FastRandom(seed * 7919L + 17),
                    TreeRecorder.BASE_X, TreeRecorder.BASE_Y, TreeRecorder.BASE_Z);
            if (!columns.isEmpty()) {
                footprint += columns.size();
                bottoms.add(range[0]);
                tops.add(range[1]);
            }
        }
        if (leaf[0] == null) {
            return new Crown(-1, 0, 0, 0);
        }
        bottoms.sort(null);
        tops.sort(null);
        return new Crown(palette.idOf(leaf[0]), footprint / bottoms.size(),
                bottoms.get(bottoms.size() / 2), tops.get(tops.size() / 2));
    }

    /** The tree blocks of a node, by sampled cell. */
    static final class Cells {
        private final Map<Long, Integer> materials = new HashMap<>();
        private int trees;

        /** @return the palette number of the tree block in this cell, or -1 */
        int material(int u, int v, int y) {
            Integer material = materials.get(key(u, v, y));
            return material == null ? -1 : material;
        }

        boolean isEmpty() {
            return materials.isEmpty();
        }

        int trees() {
            return trees;
        }

        private static long key(int u, int v, int y) {
            return ((long) u << 42) | ((long) v << 21) | (y & 0x1FFFFF);
        }
    }

    /**
     * @param minX the world column of the node's first column
     * @param minZ likewise
     * @param size columns along a side
     * @param stride blocks per column
     * @param lowestGround the lowest ground a tree could stand on, in blocks
     * @param highestGround the highest
     */
    Cells plant(int minX, int minZ, int size, int stride, int lowestGround, int highestGround) {
        Cells cells = new Cells();
        if (highestGround < lowestGround) {
            return cells;
        }
        int span = size * stride;
        // A tree standing outside the node reaches into it by up to its radius.
        BlockRegion area = new BlockRegion(minX - REACH_SIDEWAYS, lowestGround + 1, minZ - REACH_SIDEWAYS)
                .setSize(span + 2 * REACH_SIDEWAYS, highestGround - lowestGround + 1, span + 2 * REACH_SIDEWAYS);
        Region region = world.getWorldData(area, 1);
        TreeFacet facet = region.getFacet(TreeFacet.class);
        if (facet == null) {
            return cells;
        }
        TreeRecorder recorder = new TreeRecorder();
        for (Map.Entry<Vector3ic, TreeGenerator> entry : facet.getWorldEntries().entrySet()) {
            Vector3ic base = entry.getKey();
            if (!area.contains(base)) {
                continue;
            }
            cells.trees++;
            recorder.into((dx, dy, dz, block) -> sample(cells, block, base.x() + dx, base.y() + dy, base.z() + dz,
                    minX, minZ, size, stride));
            // The seed the rasterizer gives it, from the world position of its base.
            int seed = new Vector3i(base).hashCode();
            entry.getValue().generate(blockManager, recorder, new FastRandom(seed),
                    TreeRecorder.BASE_X, TreeRecorder.BASE_Y, TreeRecorder.BASE_Z);
        }
        return cells;
    }

    private void sample(Cells cells, Block block, int x, int y, int z, int minX, int minZ, int size, int stride) {
        if (Math.floorMod(x - minX, stride) != 0 || Math.floorMod(z - minZ, stride) != 0
                || Math.floorMod(y, stride) != 0) {
            return;
        }
        int u = Math.floorDiv(x - minX, stride);
        int v = Math.floorDiv(z - minZ, stride);
        if (u < 0 || v < 0 || u >= size || v >= size) {
            return;
        }
        cells.materials.put(Cells.key(u, v, Math.floorDiv(y, stride)), palette.idOf(block));
    }

    /** How far above its ground a tree reaches, for the heights a node has to keep. */
    static int reachUp() {
        return REACH_UP;
    }
}
