// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.joml.Vector2ic;
import org.joml.Vector3f;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.cubeworlds.generator.facets.CaveFacet;
import org.terasology.cubeworlds.topology.CubePattern;
import org.terasology.engine.context.Context;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.logic.console.Console;
import org.terasology.engine.logic.console.commandSystem.annotations.Command;
import org.terasology.engine.logic.console.commandSystem.annotations.CommandParam;
import org.terasology.engine.logic.permission.PermissionManager;
import org.terasology.engine.logic.players.LocalPlayer;
import org.terasology.engine.registry.In;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.chunks.Chunks;
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.facets.ElevationFacet;
import org.terasology.engine.world.generator.WorldGenerator;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Deque;
import java.util.List;
import java.util.Locale;

/**
 * Finds the cube world's structures on the ground and counts them, without loading a single chunk.
 * <p>
 * <b>Asked of the generator, not of the loaded world.</b> The running game only holds what the view
 * distance reaches — nine chunks, 288 blocks — and a cave mouth is a rare feature: counting them in
 * the blocks around the player would answer a question about the renderer's budget, not about the
 * world. The facet chain answers anywhere, at any distance, for the cost of the passes involved and
 * nothing else. Nothing is generated, meshed, lit or saved.
 * <p>
 * <b>The only structure this generator makes is a cave mouth.</b> There are no villages, no ruins
 * and no dungeons in {@code CubeWorldGenerator}'s chain, so a registry of structure kinds would be a
 * table with one row in it. The report is nonetheless written as a table by kind: adding a kind is
 * one private method returning its {@link Found} list, and one line in {@link #locateStructures}.
 * <p>
 * <b>Two queries per tile, and that is the cheap way round.</b> The caves are a three dimensional
 * facet, so asking for them means naming a band of height; the band that matters is the one the
 * ground runs through, and only the elevation knows where that is. Asking for the caves over the
 * whole world instead would be five hundred blocks of density per column to read six.
 * <p>
 * <b>It blocks the game while it runs.</b> The cave pass is the expensive one, and this runs on the
 * main thread like every other console command: 71 chunks of ground took 2,6 seconds when measured,
 * so the radius is capped, for that reason and for no other.
 */
@RegisterSystem
public class CubeStructureCommands extends BaseComponentSystem {

    /**
     * Columns per tile: one chunk, the unit the facet chain is priced in. Thirty-two here, not
     * sixteen — a chunk of this engine is 32 by 64 by 32, and a budget written against sixteen is
     * out by a factor of four, which is a ceiling that never stops anything.
     */
    private static final int TILE = Chunks.SIZE_X;

    /** Default half-width of the scan, in blocks. Nine tiles a side, measured at 2,6 seconds. */
    private static final int DEFAULT_RADIUS = 128;

    /**
     * Ceiling on the number of tiles. A tile is a cave pass and costs about 36 ms, measured on the
     * seed this was written against: 256 tiles is some nine seconds of frozen game, and a console
     * command that freezes the game for longer reads as a crash — the player has no way to tell the
     * two apart.
     */
    private static final int MAX_TILES = 256;

    /**
     * Rock left above and below the ground line when asking for the caves.
     * <p>
     * One block each way would do for the test itself, but the elevation is a float and the
     * rasterizer rounds it; a band that is exactly the ground line would put the cell of interest on
     * the border of the facet on half the columns.
     */
    private static final int BAND_MARGIN = 3;

    /** How far above the reference level a column has to stand before it can hold a mouth. */
    private static final int DRY_MARGIN = 1;

    @In
    private Context context;

    @In
    private LocalPlayer localPlayer;

    @In
    private WorldGenerator worldGenerator;

    @Command(shortDescription = "Locates the cube world's surface structures around the player",
            helpText = "Asks the generator — not the loaded chunks — which structures stand within "
                    + "the given radius in blocks, and reports them by kind. The only kind this "
                    + "generator makes is a cave mouth: a column of ground the galleries have "
                    + "broken through, so that one can walk in from the open air. Freezes the game "
                    + "while it runs — about 36 ms per chunk of ground — so the radius is capped "
                    + "at 256 blocks.",
            requiredPermission = PermissionManager.CHEAT_PERMISSION)
    public String locateStructures(
            @CommandParam(value = "radius", required = false) Integer radiusParam) {

        CubeGeometry geometry = context.get(CubeGeometry.class);
        if (geometry == null) {
            return "this world was not made by the cube generator: there is no cube geometry to "
                    + "ask, and the structures of another generator are not this command's to find";
        }

        World world = worldGenerator.getWorld();
        Vector3f eye = localPlayer.getPosition(new Vector3f());
        int radius = radiusParam != null ? radiusParam : DEFAULT_RADIUS;
        if (radius < TILE) {
            return "a radius under " + TILE + " blocks is less than one tile of the scan";
        }

        int tileX0 = Math.floorDiv(Math.round(eye.x) - radius, TILE);
        int tileX1 = Math.floorDiv(Math.round(eye.x) + radius, TILE);
        int tileZ0 = Math.floorDiv(Math.round(eye.z) - radius, TILE);
        int tileZ1 = Math.floorDiv(Math.round(eye.z) + radius, TILE);
        int side = tileX1 - tileX0 + 1;
        if (side * side > MAX_TILES) {
            return String.format(Locale.ROOT, "a radius of %d blocks is %d tiles of generation and "
                    + "would hang the game for the best part of a minute; %d tiles is the ceiling, "
                    + "so keep the radius under %d", radius, side * side, MAX_TILES,
                    TILE * (int) Math.sqrt(MAX_TILES) / 2);
        }

        int face = geometry.faceOf(Math.round(eye.x), Math.round(eye.z));
        if (face == CubePattern.NO_FACE) {
            return "the player is standing on a dead slot of the net, which belongs to no face and "
                    + "holds no ground: move onto a face and ask again";
        }

        Scan scan = new Scan(geometry, face, tileX0, tileZ0, side * TILE);
        scan.read(world);
        List<Found> mouths = scan.cluster();

        return report(scan, mouths, radius, face);
    }

    private String report(Scan scan, List<Found> mouths, int radius, int face) {
        StringBuilder out = new StringBuilder();
        out.append(String.format(Locale.ROOT,
                "face %d, %d blocks around the player, %d tiles generated: %d columns of dry land "
                        + "out of %d scanned",
                face, radius, scan.tiles, scan.land, scan.scanned)).append(Console.NEW_LINE);
        out.append(String.format(Locale.ROOT, "%-12s %7s %s", "kind", "count", "where"))
                .append(Console.NEW_LINE);
        out.append(String.format(Locale.ROOT, "%-12s %7d %s", "caveMouth", mouths.size(),
                mouths.isEmpty() ? "-" : "listed below")).append(Console.NEW_LINE);

        if (mouths.isEmpty()) {
            return out.append("no gallery reaches the open air here. The generator seals a gallery "
                    + "under six blocks of rock wherever its entrance field forbids one, so a "
                    + "stretch of coast with none is expected; a whole face with none is not.")
                    .toString();
        }

        // Largest first: a mouth of one column is a pock in the ground, a mouth of forty is a hole
        // one walks into, and the difference is what the player cares about.
        mouths.sort((a, b) -> Integer.compare(b.columns, a.columns));
        int listed = Math.min(mouths.size(), 10);
        for (int rank = 0; rank < listed; rank++) {
            Found mouth = mouths.get(rank);
            out.append(String.format(Locale.ROOT, "  caveMouth  %5d %4d %5d   %3d columns",
                    mouth.x, mouth.ground, mouth.z, mouth.columns)).append(Console.NEW_LINE);
        }
        if (mouths.size() > listed) {
            out.append(String.format(Locale.ROOT, "  ... and %d smaller", mouths.size() - listed))
                    .append(Console.NEW_LINE);
        }
        out.append(String.format(Locale.ROOT, "one mouth per %d columns of dry land",
                mouths.isEmpty() ? 0 : scan.land / mouths.size()));
        return out.toString();
    }

    /** One located structure: where it is, how wide, and how high the ground stands there. */
    private static final class Found {
        private final int x;
        private final int z;
        private final int columns;
        private final int ground;

        private Found(int x, int z, int columns, int ground) {
            this.x = x;
            this.z = z;
            this.columns = columns;
            this.ground = ground;
        }
    }

    /** The grid of columns being scanned, and what was read into it. */
    private static final class Scan {
        private final CubeGeometry geometry;
        private final int face;
        private final int originX;
        private final int originZ;
        private final int width;
        private final boolean[] open;
        private final int[] ground;
        private int scanned;
        private int land;
        private int tiles;

        private Scan(CubeGeometry geometry, int face, int tileX0, int tileZ0, int width) {
            this.geometry = geometry;
            this.face = face;
            this.originX = tileX0 * TILE;
            this.originZ = tileZ0 * TILE;
            this.width = width;
            this.open = new boolean[width * width];
            this.ground = new int[width * width];
        }

        private void read(World world) {
            for (int x = originX; x < originX + width; x += TILE) {
                for (int z = originZ; z < originZ + width; z += TILE) {
                    readTile(world, x, z);
                }
            }
        }

        private void readTile(World world, int tileX, int tileZ) {
            BlockRegion columns = new BlockRegion(tileX, geometry.seaLevel(), tileZ)
                    .setSize(TILE, 1, TILE);
            ElevationFacet elevation = world.getWorldData(columns).getFacet(ElevationFacet.class);

            int lowest = Integer.MAX_VALUE;
            int highest = Integer.MIN_VALUE;
            for (Vector2ic column : elevation.getWorldArea()) {
                if (!onFace(column.x(), column.y())) {
                    continue;
                }
                int top = topOf(elevation.getWorld(column));
                if (top < geometry.seaLevel() + DRY_MARGIN) {
                    continue;
                }
                lowest = Math.min(lowest, top);
                highest = Math.max(highest, top);
            }
            if (lowest > highest) {
                // Every column of the tile is sea, or off the face. No caves to ask for: the
                // generator seals the sea floor, and this is where most of the scan's budget is
                // saved on a coastal world. The columns are still counted, or the denominator of
                // the report would quietly shrink by however much ocean the square holds.
                for (int x = tileX; x < tileX + TILE; x++) {
                    for (int z = tileZ; z < tileZ + TILE; z++) {
                        if (onFace(x, z)) {
                            scanned++;
                        }
                    }
                }
                return;
            }

            BlockRegion band = new BlockRegion(tileX, lowest - BAND_MARGIN, tileZ)
                    .setSize(TILE, highest - lowest + 2 * BAND_MARGIN + 1, TILE);
            Region region = world.getWorldData(band);
            CaveFacet caves = region.getFacet(CaveFacet.class);
            tiles++;

            for (int x = tileX; x < tileX + TILE; x++) {
                for (int z = tileZ; z < tileZ + TILE; z++) {
                    if (!onFace(x, z)) {
                        continue;
                    }
                    scanned++;
                    int top = topOf(elevation.getWorld(x, z));
                    if (top < geometry.seaLevel() + DRY_MARGIN) {
                        continue;
                    }
                    land++;
                    ground[index(x, z)] = top;
                    // Two cells, not one: a single carved cell at the ground line is a dimple the
                    // flora will stand in, and counting those would drown the real mouths.
                    if (caves.getWorld(x, top, z) && caves.getWorld(x, top - 1, z)) {
                        open[index(x, z)] = true;
                    }
                }
            }
        }

        /**
         * The topmost cell a cave could ever have taken out of a column, which is a floor and never
         * a rounding. The provider only looks at cells where the density is positive, and the
         * density is exactly {@code elevation - y}: at an elevation of 36,7 the cell at 37 was never
         * eligible for carving. Rounding asked about it anyway, and so reported every column whose
         * elevation has a fraction of a half or more as closed — half the world, silently.
         */
        private static int topOf(float elevation) {
            return (int) Math.floor(elevation);
        }

        /**
         * On the player's face and nowhere else. The facet carries a border that reaches past the
         * tile, and past the edge of a face it lands either on the neighbour or on a dead slot of
         * the net, where {@code CubeSurfaceProvider} writes an elevation a hundred blocks under the
         * sea. Counting those would put the denominator of the report out by the border.
         */
        private boolean onFace(int x, int z) {
            return geometry.faceOf(x, z) == face
                    && x >= originX && x < originX + width
                    && z >= originZ && z < originZ + width;
        }

        private int index(int x, int z) {
            return (x - originX) * width + (z - originZ);
        }

        /**
         * One mouth is many columns. A flood fill over the four neighbours turns the field of open
         * columns into the holes a player would count walking over the ground; counting columns
         * instead would report a single cave entrance as forty structures.
         */
        private List<Found> cluster() {
            List<Found> found = new ArrayList<>();
            boolean[] seen = new boolean[open.length];
            Deque<Integer> queue = new ArrayDeque<>();
            for (int start = 0; start < open.length; start++) {
                if (!open[start] || seen[start]) {
                    continue;
                }
                seen[start] = true;
                queue.add(start);
                long sumX = 0;
                long sumZ = 0;
                long sumGround = 0;
                int columns = 0;
                while (!queue.isEmpty()) {
                    int cell = queue.remove();
                    int cx = cell / width;
                    int cz = cell % width;
                    columns++;
                    sumX += originX + cx;
                    sumZ += originZ + cz;
                    sumGround += ground[cell];
                    offer(seen, queue, cx - 1, cz);
                    offer(seen, queue, cx + 1, cz);
                    offer(seen, queue, cx, cz - 1);
                    offer(seen, queue, cx, cz + 1);
                }
                found.add(new Found((int) (sumX / columns), (int) (sumZ / columns), columns,
                        (int) (sumGround / columns)));
            }
            return found;
        }

        private void offer(boolean[] seen, Deque<Integer> queue, int cx, int cz) {
            if (cx < 0 || cz < 0 || cx >= width || cz >= width) {
                return;
            }
            int cell = cx * width + cz;
            if (open[cell] && !seen[cell]) {
                seen[cell] = true;
                queue.add(cell);
            }
        }
    }
}
