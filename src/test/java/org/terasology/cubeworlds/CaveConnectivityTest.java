// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.joml.Vector3ic;
import org.junit.jupiter.api.Test;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.core.world.generator.facetProviders.SurfaceToDensityProvider;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.facetProviders.CubeCaveProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeRegionProvider;
import org.terasology.cubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.cubeworlds.generator.facets.CaveFacet;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;

import java.util.Arrays;

import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Whether the caves are a network or a scattering of sealed bubbles.
 * <p>
 * This is the test the rest of the suite cannot stand in for. A volume census reads the same on a
 * connected system of galleries and on the same volume broken into ten thousand pockets, and a
 * screenshot of a cave one is standing in says nothing about whether it leads anywhere. Sealed
 * pockets are the worst outcome available — they are meshed, lit and saved for the life of the
 * world, and never seen — and the way to get them is an octave of noise about the size of the
 * threshold, which is a one character change away at all times.
 * <p>
 * The truncation bias runs the safe way. Cutting a box out of the world can only break a component
 * in two, never join two that were apart, so the share measured here is a <em>lower bound</em> on
 * the real one — which is the direction an assertion of "at least" needs.
 */
class CaveConnectivityTest {

    private static final int SPAN = 128;
    private static final int LOW = -176;
    private static final int HIGH = 16;
    private static final int HEIGHT = HIGH - LOW;

    @Test
    void theGalleriesJoinUpIntoANetwork() {
        CubeGeometry geometry = CubeGeometry.createDefault();
        boolean[] carved = carveABox(geometry);

        int[] label = new int[carved.length];
        int[] stack = new int[carved.length];
        int components = 0;
        int largest = 0;
        int second = 0;
        int bigEnough = 0;
        int total = 0;

        for (int index = 0; index < carved.length; index++) {
            if (!carved[index] || label[index] != 0) {
                continue;
            }
            components++;
            int size = 0;
            int top = 0;
            stack[top++] = index;
            label[index] = components;
            while (top > 0) {
                int at = stack[--top];
                size++;
                for (int neighbour : neighbours(at)) {
                    if (neighbour >= 0 && carved[neighbour] && label[neighbour] == 0) {
                        label[neighbour] = components;
                        stack[top++] = neighbour;
                    }
                }
            }
            total += size;
            if (size >= 512) {
                bigEnough++;
            }
            if (size > largest) {
                second = largest;
                largest = size;
            } else if (size > second) {
                second = size;
            }
        }

        double headroom = runsOfAtLeast(carved, 2) / (double) Math.max(1, total);
        double halls = runsOfAtLeast(carved, 5) / (double) Math.max(1, total);
        System.out.printf("%d carved cells in %d components; largest %.1f %%, second %.1f %%, "
                        + "%d components of 512 cells or more%n",
                total, components, 100.0 * largest / total, 100.0 * second / total, bigEnough);

        // The height profile, which is the only thing that says whether the caves are walked or
        // crawled. A cell counts only if the cells above it are carved too, so a gallery of height h
        // can never score better than (h-1)/h at the two-block mark: the figure is a ceiling on the
        // shape, not a percentage of anything a player counts.
        System.out.print("  standing room:");
        for (int tall : new int[]{2, 3, 5, 10, 16, 24}) {
            System.out.printf("  %d+ %.1f %%", tall, 100.0 * runsOfAtLeast(carved, tall) / total);
        }
        System.out.println();

        assertTrue(total > 20000, "too little was carved in this box for the shape to mean anything");
        assertTrue(largest > 0.5 * total,
                "the caves are not a network: the largest component holds only "
                        + (100.0 * largest / total) + " % of the carved cells");

        // What this figure can be, so that it is read for what it is. A cell counts only if the
        // cell above it is carved too, so a gallery of height h can never score better than
        // (h-1)/h: 67 % at three blocks, 50 % at two. Both ends of every gallery also taper to
        // nothing, by design, and those cells never count. Fifty-odd per cent is therefore an
        // average run of a little over two blocks against a character of 1.6 — passable
        // throughout, roomy in the main system — and not a suite of cracks.
        assertTrue(headroom > 0.50,
                "most of what was carved is a crack rather than a passage: only "
                        + (100 * headroom) + " % of it stands two blocks tall");
        // The rooms are a swelling of the galleries and nothing else creates height, so this is
        // the only evidence that the swell does anything at all.
        assertTrue(halls > 0.02,
                "nothing opened out into a hall: the room swell is not reaching the radius");
    }

    /** @return the six neighbours of a cell, with -1 where the box ends */
    private static int[] neighbours(int index) {
        int x = index % SPAN;
        int y = (index / SPAN) % HEIGHT;
        int z = index / (SPAN * HEIGHT);
        return new int[]{
                x > 0 ? index - 1 : -1,
                x < SPAN - 1 ? index + 1 : -1,
                y > 0 ? index - SPAN : -1,
                y < HEIGHT - 1 ? index + SPAN : -1,
                z > 0 ? index - SPAN * HEIGHT : -1,
                z < SPAN - 1 ? index + SPAN * HEIGHT : -1};
    }

    /** @return carved cells with at least {@code tall} carved cells stacked from them upwards */
    private static int runsOfAtLeast(boolean[] carved, int tall) {
        int count = 0;
        for (int index = 0; index < carved.length; index++) {
            if (!carved[index]) {
                continue;
            }
            int y = (index / SPAN) % HEIGHT;
            if (y + tall > HEIGHT) {
                continue;
            }
            boolean clear = true;
            for (int step = 1; step < tall && clear; step++) {
                clear = carved[index + step * SPAN];
            }
            if (clear) {
                count++;
            }
        }
        return count;
    }

    private static boolean[] carveABox(CubeGeometry geometry) {
        WorldBuilder builder = new WorldBuilder(null)
                .setSeaLevel(geometry.seaLevel())
                .addProvider(new SeaLevelProvider(geometry.seaLevel()))
                .addProvider(new CubeSurfaceProvider(geometry))
                .addProvider(new CubeClimateProvider(geometry))
                .addProvider(new CubeRegionProvider(geometry))
                .addProvider(new SurfaceToDensityProvider())
                .addProvider(new CubeCaveProvider(geometry));
        builder.setSeed(0xC0FFEE);
        World world = builder.build();

        int edge = geometry.faceBlocks();
        int originX = CubePattern.netX(1, edge / 2, edge);
        int originZ = CubePattern.netY(1, edge / 2, edge);

        boolean[] carved = new boolean[SPAN * HEIGHT * SPAN];
        Arrays.fill(carved, false);
        for (int dx = 0; dx < SPAN; dx += 32) {
            for (int dz = 0; dz < SPAN; dz += 32) {
                for (int y = LOW; y < HIGH; y += 64) {
                    BlockRegion request = new BlockRegion(originX + dx, y, originZ + dz).setSize(32, 64, 32);
                    Region region = world.getWorldData(request);
                    CaveFacet caves = region.getFacet(CaveFacet.class);
                    for (Vector3ic pos : request) {
                        if (caves.getWorld(pos)) {
                            carved[index(pos.x() - originX, pos.y() - LOW, pos.z() - originZ)] = true;
                        }
                    }
                }
            }
        }
        return carved;
    }

    private static int index(int x, int y, int z) {
        return x + SPAN * (y + HEIGHT * z);
    }
}
