// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.joml.Vector2i;
import org.joml.Vector3f;
import org.terasology.biomesAPI.Biome;
import org.terasology.core.world.generator.facets.BiomeFacet;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.topology.CubePattern;
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
import org.terasology.engine.world.generation.Region;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generation.facets.ElevationFacet;
import org.terasology.engine.world.generator.WorldGenerator;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.TreeMap;

/**
 * Finds the nearest columns of a biome over the whole planet, without loading a single chunk.
 * <p>
 * Asked of the generator, like {@link CubeStructureCommands}, and at a coarse scale: the biomes are
 * bands and regions hundreds of blocks across, so one column every {@link #STRIDE} blocks finds every
 * one of them, and the six faces cost a fraction of a second. The answer is a place to teleport to,
 * with the ground height read off the same pass, not a map.
 */
@RegisterSystem
public class CubeBiomeCommands extends BaseComponentSystem {

    /** Blocks between two sampled columns. */
    private static final int STRIDE = 32;

    /** How many places are listed, nearest first. */
    private static final int LISTED = 5;

    @In
    private Context context;

    @In
    private LocalPlayer localPlayer;

    @In
    private WorldGenerator worldGenerator;

    @Command(shortDescription = "Trouve les endroits les plus proches d'un biome",
            helpText = "Interroge le générateur sur les six faces, une colonne tous les 32 blocs, et donne "
                    + "les cinq endroits les plus proches où pousse le biome nommé (Volcanic, Pack Ice, "
                    + "Ice Shelf, Abyss…), avec la hauteur du sol pour s'y téléporter. Sans nom, liste "
                    + "les biomes et leur part de la planète.",
            runOnServer = true,
            requiredPermission = PermissionManager.CHEAT_PERMISSION)
    public String trouverBiome(@CommandParam(value = "biome", required = false) String name) {
        CubeGeometry geometry = context.get(CubeGeometry.class);
        if (geometry == null) {
            return "ce monde n'a pas été fait par le générateur cubique : il n'y a pas de géométrie à interroger";
        }
        World world = worldGenerator.getWorld();
        Vector3f eye = localPlayer.getPosition(new Vector3f());
        String wanted = name == null ? null : normalise(name);

        List<Place> found = new ArrayList<>();
        Map<String, Integer> census = new TreeMap<>();
        int sampled = 0;
        int side = geometry.faceBlocks() / STRIDE;
        Vector2i cell = new Vector2i();
        for (int face = 0; face < CubePattern.FACES; face++) {
            int originX = CubePattern.netX(face, 0, geometry.faceBlocks());
            int originZ = CubePattern.netY(face, 0, geometry.faceBlocks());
            BlockRegion cells = new BlockRegion(originX / STRIDE, Math.floorDiv(geometry.seaLevel(), STRIDE),
                    originZ / STRIDE).setSize(side, 1, side);
            Region region = world.getWorldData(cells, STRIDE);
            BiomeFacet biomes = region.getFacet(BiomeFacet.class);
            ElevationFacet elevation = region.getFacet(ElevationFacet.class);
            for (int v = 0; v < side; v++) {
                for (int u = 0; u < side; u++) {
                    cell.set(cells.minX() + u, cells.minZ() + v);
                    int x = cell.x * STRIDE;
                    int z = cell.y * STRIDE;
                    if (geometry.faceOf(x, z) != face) {
                        continue;
                    }
                    Biome biome = biomes.getWorld(cell);
                    if (biome == null) {
                        continue;
                    }
                    sampled++;
                    census.merge(biome.getId().toString(), 1, Integer::sum);
                    if (wanted != null && matches(biome, wanted)) {
                        int ground = Math.round(elevation.getWorld(cell));
                        found.add(new Place(x, ground, z, face, geometry.arc(eye.x, eye.z, x, z)));
                    }
                }
            }
        }

        StringBuilder out = new StringBuilder();
        if (wanted == null) {
            out.append(String.format(Locale.ROOT, "%d colonnes échantillonnées sur les six faces", sampled))
                    .append(Console.NEW_LINE);
            for (Map.Entry<String, Integer> entry : census.entrySet()) {
                out.append(String.format(Locale.ROOT, "  %-22s %5.1f %%", entry.getKey(),
                        100.0 * entry.getValue() / Math.max(1, sampled))).append(Console.NEW_LINE);
            }
            return out.toString();
        }
        if (found.isEmpty()) {
            return "aucune colonne de « " + name + " » sur la planète. Biomes présents : "
                    + String.join(", ", census.keySet());
        }
        found.sort((a, b) -> Double.compare(a.distance, b.distance));
        out.append(String.format(Locale.ROOT, "%s : %.1f %% de la planète, les %d plus proches :", name,
                100.0 * found.size() / Math.max(1, sampled), Math.min(LISTED, found.size())))
                .append(Console.NEW_LINE);
        for (int rank = 0; rank < Math.min(LISTED, found.size()); rank++) {
            Place place = found.get(rank);
            out.append(String.format(Locale.ROOT, "  teleport %d %d %d   (face %s, à %.0f blocs)",
                    place.x, Math.max(place.ground, geometry.seaLevel()) + 3, place.z,
                    CubePattern.NAMES[place.face], place.distance)).append(Console.NEW_LINE);
        }
        return out.toString();
    }

    /** Whether a biome answers to a name, with or without the module in front of it. */
    private static boolean matches(Biome biome, String wanted) {
        String id = biome.getId().toString();
        return normalise(id).equals(wanted) || normalise(id.substring(id.indexOf(':') + 1)).equals(wanted);
    }

    /** Case and spaces aside, so that « pack ice », « PACK_ICE » and « Pack Ice » are one name. */
    private static String normalise(String name) {
        return name.toLowerCase(Locale.ROOT).replaceAll("[^a-z0-9]", "");
    }

    private static final class Place {
        private final int x;
        private final int ground;
        private final int z;
        private final int face;
        private final double distance;

        private Place(int x, int ground, int z, int face, double distance) {
            this.x = x;
            this.ground = ground;
            this.z = z;
            this.face = face;
            this.distance = distance;
        }
    }
}
