// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.joml.Vector3i;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import org.terasology.biomesAPI.Biome;
import org.terasology.core.world.CoreBiome;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.core.world.generator.facetProviders.SurfaceToDensityProvider;
import org.terasology.herocubeworlds.distant.ColumnSection;
import org.terasology.herocubeworlds.distant.ColumnSummarizer;
import org.terasology.herocubeworlds.distant.SectionKey;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.facetProviders.CubeBiomeProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeCaveProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeRegionProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.herocubeworlds.generator.rasterizers.CubeCell;
import org.terasology.herocubeworlds.topology.CubeFolding;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;

import java.util.Locale;
import java.util.TreeMap;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * What the distant terrain keeps of the world, level by level, measured against the world itself.
 * <p>
 * The truth is level 0: one column per block, the full chain, galleries included — the columns a
 * chunk would hold, minus the ores and the plants. A coarser level is summarised straight from the
 * generator at its own scale and read against that truth on the same columns of the world: how far
 * the ground moved, how many of the overhangs near the surface survived, what a node weighs and
 * what it costs to make. None of it needs the engine.
 * <p>
 * An overhang is counted where it can be seen from afar: air under solid ground within
 * {@link #OVERHANG_DEPTH} blocks of the top of the column. A gallery a hundred blocks down is not a
 * loss of the distant terrain, and counting it would make every level look worse than it is.
 */
class DistantColumnCensusTest {

    private static final int OVERHANG_DEPTH = 32;
    private static final int[] LEVELS = {1, 2, 3, 4};

    private static CubeGeometry geometry;
    private static ColumnSummarizer summarizer;

    @BeforeAll
    static void buildChain() {
        geometry = CubeGeometry.createDefault();
        WorldBuilder builder = new WorldBuilder(null)
                .setSeaLevel(geometry.seaLevel())
                .addProvider(new SeaLevelProvider(geometry.seaLevel()))
                .addProvider(new CubeSurfaceProvider(geometry))
                .addProvider(new CubeClimateProvider(geometry))
                .addProvider(new CubeRegionProvider(geometry))
                .addProvider(new CubeBiomeProvider())
                .addProvider(new SurfaceToDensityProvider())
                .addProvider(new CubeCaveProvider(geometry));
        builder.setSeed(0xC0FFEE);
        World world = builder.build();
        summarizer = new ColumnSummarizer(world, geometry, DistantColumnCensusTest::materialOf);
    }

    /**
     * The census names a material by what decides it in the rasterizer: the kind of cell, the
     * biome, and for the ground under the surface which side of the soil, rock and hard rock
     * frontiers it lies on — the two published frontiers of {@link CoreBiome}, not a copy of them.
     */
    private static int materialOf(CubeCell cell, Biome biome, Vector3i pos, int seaLevel, float density) {
        int band = 0;
        if (cell == CubeCell.BELOW_SURFACE && biome instanceof CoreBiome) {
            band = density > CoreBiome.HARD_ROCK_DEPTH ? 2 : density > ((CoreBiome) biome).rockDepth() ? 1 : 0;
        }
        int biomeId = biome instanceof CoreBiome ? ((CoreBiome) biome).ordinal() : 63;
        return (cell.ordinal() << 8) | (biomeId << 2) | band;
    }

    /** Where the census looks: the middle of a face, the edge of a cut seam, and a corner. */
    private static SectionKey[] placesAt(int level) {
        int n = SectionKey.nodesPerSide(level);
        return new SectionKey[]{
                new SectionKey(1, level, n / 2, n / 2),
                new SectionKey(4, level, 0, n / 2),
                new SectionKey(1, level, 0, n - 1)
        };
    }

    @Test
    void theDistantTerrainKeepsTheGroundAndSaysWhatItLoses() {
        int size = SectionKey.columns(geometry);
        System.out.printf(Locale.ROOT, "colonnes par cote de noeud : %d%n", size);
        System.out.printf(Locale.ROOT, "%6s %7s | %8s %8s %8s | %10s %9s | %8s %9s %11s %11s%n",
                "niveau", "colonnes", "ecart", "max", "emprise", "surplombs", "perdus", "seg/col",
                "ms/noeud", "Mo/face", "Mo/planete");
        for (int level : LEVELS) {
            int stride = 1 << level;
            double sumPoint = 0;
            double worstPoint = 0;
            double sumFootprint = 0;
            int compared = 0;
            int truthOverhangs = 0;
            int keptOverhangs = 0;
            long bytes = 0;
            long segments = 0;
            long nanos = 0;
            int nodes = 0;
            for (SectionKey coarseKey : placesAt(level)) {
                long start = System.nanoTime();
                ColumnSection coarse = summarizer.summarize(coarseKey);
                nanos += System.nanoTime() - start;
                nodes++;
                bytes += coarse.bytes();
                segments += coarse.segmentCount();

                // The truth: the level-0 node in the corner of the coarse one, the same columns of
                // the world, read at one block per column.
                SectionKey fineKey = new SectionKey(coarseKey.face(), 0, coarseKey.i() * stride, coarseKey.j() * stride);
                ColumnSection fine = summarizer.summarize(fineKey);
                int covered = size / stride;
                for (int v = 0; v < covered; v++) {
                    for (int u = 0; u < covered; u++) {
                        int coarseTop = topOf(coarse, u, v);
                        int fineTop = topOf(fine, u * stride, v * stride);
                        if (coarseTop == Integer.MIN_VALUE || fineTop == Integer.MIN_VALUE) {
                            continue;
                        }
                        double error = Math.abs(coarseTop - fineTop);
                        sumPoint += error;
                        worstPoint = Math.max(worstPoint, error);
                        sumFootprint += Math.abs(coarseTop - meanTop(fine, u * stride, v * stride, stride));
                        compared++;
                        if (hasOverhang(fine, u * stride, v * stride)) {
                            truthOverhangs++;
                            if (hasOverhang(coarse, u, v)) {
                                keptOverhangs++;
                            }
                        }
                    }
                }
            }
            double perNode = (double) bytes / nodes;
            int nodesPerFace = SectionKey.nodesPerSide(level) * SectionKey.nodesPerSide(level);
            double faceMb = perNode * nodesPerFace / (1 << 20);
            double lost = truthOverhangs == 0 ? 0 : 1.0 - (double) keptOverhangs / truthOverhangs;
            System.out.printf(Locale.ROOT, "%6d %7d | %8.2f %8.1f %8.2f | %10d %8.1f%% | %8.2f %9.1f %11.2f %11.2f%n",
                    level, compared, sumPoint / compared, worstPoint, sumFootprint / compared,
                    truthOverhangs, 100 * lost, (double) segments / (nodes * size * size),
                    nanos / 1e6 / nodes, faceMb, 6 * faceMb);
            assertTrue(compared > 0, "nothing was compared at level " + level);
            // The ground may land anywhere in one coarse cell, never further on average.
            assertTrue(sumPoint / compared <= stride, "the ground moved by more than a cell at level " + level);
        }
    }

    /**
     * Four children for every node, on every face and at every level — the corners of the cube are
     * where the neighbours of a node go wrong, never its children — and the neighbours counted, not
     * assumed: at every level where a face has more than one node, twenty-four have seven.
     */
    @Test
    void everyNodeHasFourChildrenAndTheCornersAreCounted() {
        for (int level = SectionKey.TOP_LEVEL; level >= 1; level--) {
            int n = SectionKey.nodesPerSide(level);
            for (int face = 0; face < 6; face++) {
                for (int i = 0; i < n; i++) {
                    for (int j = 0; j < n; j++) {
                        assertEquals(4, new SectionKey(face, level, i, j).children().length);
                    }
                }
            }
        }
        System.out.printf(Locale.ROOT, "%6s %8s  %s%n", "niveau", "noeuds", "voisins -> noeuds");
        for (int level = SectionKey.TOP_LEVEL; level >= 0; level--) {
            int n = SectionKey.nodesPerSide(level);
            TreeMap<Integer, Integer> histogram = new TreeMap<>();
            for (int face = 0; face < 6; face++) {
                for (int i = 0; i < n; i++) {
                    for (int j = 0; j < n; j++) {
                        int netX = CubePattern.netX(face, i, n);
                        int netY = CubePattern.netY(face, j, n);
                        histogram.merge(CubeFolding.neighbours(netX, netY, n).length, 1, Integer::sum);
                    }
                }
            }
            System.out.printf(Locale.ROOT, "%6d %8d  %s%n", level, 6 * n * n, histogram);
            if (n > 1) {
                assertEquals(24, histogram.getOrDefault(7, 0), "corner nodes with seven neighbours at level " + level);
            }
        }
    }

    /** The top of the highest opaque run of a column. */
    private static int topOf(ColumnSection section, int u, int v) {
        for (int k = 0; k < section.count(u, v); k++) {
            long s = section.segment(u, v, k);
            if (!ColumnSection.isLiquid(s)) {
                return section.top(s);
            }
        }
        return Integer.MIN_VALUE;
    }

    private static double meanTop(ColumnSection fine, int u0, int v0, int stride) {
        double sum = 0;
        int n = 0;
        for (int v = v0; v < v0 + stride && v < fine.size(); v++) {
            for (int u = u0; u < u0 + stride && u < fine.size(); u++) {
                int top = topOf(fine, u, v);
                if (top != Integer.MIN_VALUE) {
                    sum += top;
                    n++;
                }
            }
        }
        return n == 0 ? Double.NaN : sum / n;
    }

    /** Air under opaque ground, near enough to the top of the column to be seen from afar. */
    private static boolean hasOverhang(ColumnSection section, int u, int v) {
        int top = Integer.MIN_VALUE;
        for (int k = 0; k < section.count(u, v); k++) {
            long s = section.segment(u, v, k);
            if (ColumnSection.isLiquid(s)) {
                continue;
            }
            if (top == Integer.MIN_VALUE) {
                top = section.top(s);
                continue;
            }
            // A second opaque run below a gap: the gap is air when the run above does not touch it.
            long above = section.segment(u, v, k - 1);
            boolean gap = section.bottom(above) > section.top(s) + 1;
            if (gap && section.top(s) >= top - OVERHANG_DEPTH) {
                return true;
            }
        }
        return false;
    }
}
