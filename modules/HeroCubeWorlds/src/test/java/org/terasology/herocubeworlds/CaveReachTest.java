// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.junit.jupiter.api.Test;
import org.terasology.core.world.generator.facetProviders.SeaLevelProvider;
import org.terasology.core.world.generator.facetProviders.SurfaceToDensityProvider;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.facetProviders.CubeCaveProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeClimateProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeRegionProvider;
import org.terasology.herocubeworlds.generator.facetProviders.CubeSurfaceProvider;
import org.terasology.herocubeworlds.generator.facets.CaveFacet;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.WorldBuilder;
import org.terasology.engine.world.generation.facets.ElevationFacet;

import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Deque;
import java.util.List;

/**
 * How far a player has to walk before meeting a cave they can enter. The design target is two to
 * three hundred blocks.
 * <p>
 * <b>Counting mouths does not answer it.</b> The first version of this measurement reported one
 * mouth per hundred and fifty blocks of dry land and the world still read as sealed, because that
 * figure is an area divided by a count and says nothing about where the mouths are: they clump, the
 * entrance field being noise at a hundred and twenty blocks. What a player experiences is the walk
 * from where they stand to the nearest way in, so that is what is measured — a breadth first search
 * over the dry land, from every mouth at once.
 * <p>
 * <b>And not every hole is a way in.</b> The median mouth is a few columns across; a one-column
 * hole is something one falls into, not something one sees and walks towards. Nine columns — three
 * by three — is the bar taken here for a mouth that reads as an entrance, and the headline figure is
 * the walk to one of those. Counting every pothole flatters the world by a factor of two.
 * <p>
 * Three windows of a thousand blocks over two seeds. One window is not enough: the third is the
 * hard one on this pair of seeds, and a bound set on the first two alone would pass a world where a
 * quarter of the land is out of reach.
 */
class CaveReachTest {

    private static final int TILE = 32;
    private static final int SIDE = 1024;
    private static final int UNREACHED = Integer.MAX_VALUE / 4;

    /** Columns a mouth must have before it counts as a way in rather than a pothole. */
    private static final int WALK_IN = 9;

    /** What the walk to a way in must not exceed, on average, over a window. */
    private static final int TARGET_WALK = 250;

    /** What share of the dry land must lie within three hundred blocks of a way in. */
    private static final double TARGET_COVERAGE = 0.70;

    @Test
    void aWayUndergroundIsNeverFarOff() {
        for (int[] where : new int[][]{{0xC0FFEE, 5600, 5600}, {"LUMIERE2026".hashCode(), 5600, 5600},
                {"LUMIERE2026".hashCode(), 4400, 4400}}) {
            measure(where[0], where[1], where[2]);
        }
    }

    private void measure(int seed, int centreX, int centreZ) {
        CubeGeometry geometry = CubeGeometry.createDefault();
        WorldBuilder builder = new WorldBuilder(null)
                .setSeaLevel(geometry.seaLevel())
                .addProvider(new SeaLevelProvider(geometry.seaLevel()))
                .addProvider(new CubeSurfaceProvider(geometry))
                .addProvider(new CubeClimateProvider(geometry))
                .addProvider(new CubeRegionProvider(geometry))
                .addProvider(new SurfaceToDensityProvider())
                .addProvider(new CubeCaveProvider(geometry));
        builder.setSeed(seed);
        World world = builder.build();

        int originX = centreX - SIDE / 2;
        int originZ = centreZ - SIDE / 2;
        boolean[] land = new boolean[SIDE * SIDE];
        boolean[] open = new boolean[SIDE * SIDE];

        for (int tx = originX; tx < originX + SIDE; tx += TILE) {
            for (int tz = originZ; tz < originZ + SIDE; tz += TILE) {
                readTile(world, geometry, tx, tz, originX, originZ, land, open);
            }
        }

        int landColumns = 0;
        int openColumns = 0;
        for (int i = 0; i < land.length; i++) {
            if (land[i]) {
                landColumns++;
            }
            if (open[i]) {
                openColumns++;
            }
        }
        boolean[] wide = new boolean[open.length];
        List<Integer> sizes = cluster(open, wide);
        sizes.sort((a, b) -> Integer.compare(b, a));

        int[] distance = distanceToMouth(open, land);
        int[] wideDistance = distanceToMouth(wide, land);
        long sum = 0;
        int counted = 0;
        int within100 = 0;
        int within200 = 0;
        int within300 = 0;
        int worst = 0;
        for (int i = 0; i < land.length; i++) {
            if (!land[i] || distance[i] >= UNREACHED) {
                continue;
            }
            counted++;
            sum += distance[i];
            worst = Math.max(worst, distance[i]);
            if (distance[i] <= 100) {
                within100++;
            }
            if (distance[i] <= 200) {
                within200++;
            }
            if (distance[i] <= 300) {
                within300++;
            }
        }

        System.out.printf("%n== seed %08x, %d x %d around %d,%d ==%n", seed, SIDE, SIDE, centreX, centreZ);
        System.out.printf("dry land %d columns, open %d columns, %d mouths%n",
                landColumns, openColumns, sizes.size());
        if (!sizes.isEmpty()) {
            System.out.printf("mouth sizes: largest %d, median %d, %d of them 9 columns or more%n",
                    sizes.get(0), sizes.get(sizes.size() / 2),
                    sizes.stream().filter(s -> s >= 9).count());
            System.out.printf("one mouth per %.0f blocks of dry land (square root of area each)%n",
                    Math.sqrt((double) landColumns / sizes.size()));
        }
        if (counted > 0) {
            System.out.printf("walk to the nearest mouth: mean %.0f, worst %d blocks%n",
                    (double) sum / counted, worst);
            System.out.printf("land within 100 / 200 / 300 blocks of a mouth: %.1f %% / %.1f %% / %.1f %%%n",
                    100.0 * within100 / counted, 100.0 * within200 / counted,
                    100.0 * within300 / counted);
        }
        int unreachable = landColumns - counted;
        System.out.printf("land with no mouth anywhere in the window: %d columns (%.1f %%)%n",
                unreachable, 100.0 * unreachable / Math.max(1, landColumns));

        // The figure that matters: a mouth of nine columns or more is one a player sees from a
        // distance and walks into. The rest are potholes, and counting them flatters the world.
        long wideSum = 0;
        int wideCounted = 0;
        int wideWithin300 = 0;
        int wideWorst = 0;
        for (int i = 0; i < land.length; i++) {
            if (!land[i] || wideDistance[i] >= UNREACHED) {
                continue;
            }
            wideCounted++;
            wideSum += wideDistance[i];
            wideWorst = Math.max(wideWorst, wideDistance[i]);
            if (wideDistance[i] <= 300) {
                wideWithin300++;
            }
        }
        assertTrue(wideCounted > 0,
                "not one mouth of " + WALK_IN + " columns or more in a thousand blocks of world: "
                        + "every way underground is a hole one falls into");
        double meanWalk = (double) wideSum / wideCounted;
        double coverage = (double) wideWithin300 / wideCounted;
        System.out.printf(">> walk to a mouth of %d columns or more: mean %.0f, worst %d, "
                        + "%.1f %% of land within 300 blocks (%d such mouths)%n",
                WALK_IN, meanWalk, wideWorst, 100 * coverage,
                sizes.stream().filter(s -> s >= WALK_IN).count());

        assertTrue(meanWalk < TARGET_WALK, String.format(
                "a player walks %.0f blocks on average before meeting a cave they can enter, "
                        + "against the %d asked for: seed %08x around %d,%d",
                meanWalk, TARGET_WALK, seed, centreX, centreZ));
        assertTrue(coverage > TARGET_COVERAGE, String.format(
                "only %.1f %% of the dry land is within three hundred blocks of a way in: the "
                        + "mouths have clumped, seed %08x around %d,%d",
                100 * coverage, seed, centreX, centreZ));
        // The other half of the bargain, and the one a bigger flare would break first: an entrance
        // every two hundred blocks is a landscape with caves in it, one every twenty is a sponge.
        double openShare = (double) openColumns / landColumns;
        assertTrue(openShare < 0.004, String.format(
                "the ground is pitted rather than opened: %.3f %% of land columns have a hole in "
                        + "them, seed %08x around %d,%d", 100 * openShare, seed, centreX, centreZ));
    }

    private void readTile(World world, CubeGeometry geometry, int tileX, int tileZ,
                          int originX, int originZ, boolean[] land, boolean[] open) {
        BlockRegion columns = new BlockRegion(tileX, geometry.seaLevel(), tileZ).setSize(TILE, 1, TILE);
        ElevationFacet elevation = world.getWorldData(columns).getFacet(ElevationFacet.class);

        int lowest = Integer.MAX_VALUE;
        int highest = Integer.MIN_VALUE;
        for (int x = tileX; x < tileX + TILE; x++) {
            for (int z = tileZ; z < tileZ + TILE; z++) {
                int top = topOf(elevation.getWorld(x, z));
                if (top < geometry.seaLevel() + 1) {
                    continue;
                }
                land[(x - originX) * SIDE + (z - originZ)] = true;
                lowest = Math.min(lowest, top);
                highest = Math.max(highest, top);
            }
        }
        if (lowest > highest) {
            return;
        }

        BlockRegion band = new BlockRegion(tileX, lowest - 3, tileZ)
                .setSize(TILE, highest - lowest + 7, TILE);
        Region region = world.getWorldData(band);
        CaveFacet caves = region.getFacet(CaveFacet.class);
        for (int x = tileX; x < tileX + TILE; x++) {
            for (int z = tileZ; z < tileZ + TILE; z++) {
                int cell = (x - originX) * SIDE + (z - originZ);
                if (!land[cell]) {
                    continue;
                }
                int top = topOf(elevation.getWorld(x, z));
                if (caves.getWorld(x, top, z) && caves.getWorld(x, top - 1, z)) {
                    open[cell] = true;
                }
            }
        }
    }

    /** Sizes of every mouth, and on the way, which columns belong to one of nine columns or more. */
    /**
     * The topmost cell a cave could ever have taken out of this column, which is a floor and never
     * a rounding. The provider only considers a cell where the density is positive, and the density
     * is exactly {@code elevation - y}: at an elevation of 36,7 the cell at 37 was never eligible.
     * Rounding therefore asked about a cell that is structurally always solid on every column whose
     * elevation has a fraction of a half or more — half the world, silently reading as sealed.
     */
    private static int topOf(float elevation) {
        return (int) Math.floor(elevation);
    }

    private List<Integer> cluster(boolean[] open, boolean[] wide) {
        List<Integer> sizes = new ArrayList<>();
        boolean[] seen = new boolean[open.length];
        Deque<Integer> queue = new ArrayDeque<>();
        List<Integer> members = new ArrayList<>();
        for (int start = 0; start < open.length; start++) {
            if (!open[start] || seen[start]) {
                continue;
            }
            seen[start] = true;
            queue.add(start);
            members.clear();
            while (!queue.isEmpty()) {
                int cell = queue.remove();
                members.add(cell);
                int cx = cell / SIDE;
                int cz = cell % SIDE;
                for (int[] step : new int[][]{{-1, 0}, {1, 0}, {0, -1}, {0, 1}}) {
                    int nx = cx + step[0];
                    int nz = cz + step[1];
                    if (nx < 0 || nz < 0 || nx >= SIDE || nz >= SIDE) {
                        continue;
                    }
                    int next = nx * SIDE + nz;
                    if (open[next] && !seen[next]) {
                        seen[next] = true;
                        queue.add(next);
                    }
                }
            }
            sizes.add(members.size());
            if (members.size() >= 9) {
                for (int cell : members) {
                    wide[cell] = true;
                }
            }
        }
        return sizes;
    }

    /** Breadth first over the grid from every mouth at once: the walk is over ground, not sea. */
    private int[] distanceToMouth(boolean[] open, boolean[] land) {
        int[] distance = new int[open.length];
        java.util.Arrays.fill(distance, UNREACHED);
        Deque<Integer> queue = new ArrayDeque<>();
        for (int cell = 0; cell < open.length; cell++) {
            if (open[cell]) {
                distance[cell] = 0;
                queue.add(cell);
            }
        }
        while (!queue.isEmpty()) {
            int cell = queue.remove();
            int cx = cell / SIDE;
            int cz = cell % SIDE;
            for (int[] step : new int[][]{{-1, 0}, {1, 0}, {0, -1}, {0, 1}}) {
                int nx = cx + step[0];
                int nz = cz + step[1];
                if (nx < 0 || nz < 0 || nx >= SIDE || nz >= SIDE) {
                    continue;
                }
                int next = nx * SIDE + nz;
                if (land[next] && distance[next] >= UNREACHED) {
                    distance[next] = distance[cell] + 1;
                    queue.add(next);
                }
            }
        }
        return distance;
    }
}
