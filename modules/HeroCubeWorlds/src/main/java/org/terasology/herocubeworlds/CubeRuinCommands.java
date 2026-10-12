// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.joml.Vector3f;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.CubeWorldGenerator;
import org.terasology.herocubeworlds.generator.RuinLattice;
import org.terasology.herocubeworlds.generator.facets.RuinFacet;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.logic.console.Console;
import org.terasology.engine.logic.console.commandSystem.annotations.Command;
import org.terasology.engine.logic.console.commandSystem.annotations.CommandParam;
import org.terasology.engine.logic.permission.PermissionManager;
import org.terasology.engine.logic.players.LocalPlayer;
import org.terasology.engine.registry.In;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generation.World;
import org.terasology.engine.world.generator.WorldGenerator;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * Finds the ruins around the player, asking the generator and not the loaded chunks.
 * <p>
 * Kept apart from {@code locateStructures}, which counts cave mouths tile by tile and is capped at
 * 256 blocks for the cost of the cave pass. A ruin is one candidate per cell of 640 blocks, so a scan
 * that short would come back empty most of the time; and it needs no cave pass at all — the verdict
 * reads only the elevation and the biome around the site, two cheap column facets. Asking every
 * candidate within two kilometres costs a few dozen of those.
 */
@RegisterSystem
public class CubeRuinCommands extends BaseComponentSystem {

    private static final int DEFAULT_RADIUS = 2000;
    private static final int MAX_RADIUS = 6400;
    private static final int LISTED = 10;

    @In
    private LocalPlayer localPlayer;

    @In
    private WorldGenerator worldGenerator;

    @Command(shortDescription = "Locates the ruins around the player",
            helpText = "Asks the generator which candidate sites within the radius in blocks carry "
                    + "a ruin, and lists them nearest first with their culture and the position to "
                    + "teleport to, standing on the floor of the court.",
            requiredPermission = PermissionManager.CHEAT_PERMISSION)
    public String locateRuins(@CommandParam(value = "radius", required = false) Integer radiusParam) {
        if (!(worldGenerator instanceof CubeWorldGenerator)) {
            return "this world was not made by the cube generator, and only that one builds ruins";
        }
        CubeWorldGenerator generator = (CubeWorldGenerator) worldGenerator;
        CubeGeometry geometry = generator.getGeometry();
        RuinLattice lattice = generator.getRuinLattice();
        int radius = radiusParam != null ? radiusParam : DEFAULT_RADIUS;
        if (radius < 1 || radius > MAX_RADIUS) {
            return "the radius runs from 1 to " + MAX_RADIUS + " blocks";
        }

        World world = worldGenerator.getWorld();
        Vector3f eye = localPlayer.getPosition(new Vector3f());
        int px = Math.round(eye.x);
        int pz = Math.round(eye.z);

        int candidates = 0;
        List<RuinFacet.Ruin> found = new ArrayList<>();
        for (RuinLattice.Site site : lattice.sitesNear(px, pz, px, pz, radius)) {
            candidates++;
            // The whole height of the world: the provider keeps only the ruins that reach the band
            // asked for, and a one-block band would miss every ruin whose floor is not on it.
            BlockRegion column = new BlockRegion(site.x(), geometry.floorY(), site.z())
                    .setSize(1, geometry.ceilingY() - geometry.floorY() + 1, 1);
            for (RuinFacet.Ruin ruin : world.getWorldData(column).getFacet(RuinFacet.class).getRuins()) {
                if (ruin.site().x() == site.x() && ruin.site().z() == site.z()) {
                    found.add(ruin);
                }
            }
        }
        found.sort((a, b) -> Long.compare(distance2(a, px, pz), distance2(b, px, pz)));

        StringBuilder out = new StringBuilder(String.format(Locale.ROOT,
                "%d ruins out of %d candidate sites within %d blocks", found.size(), candidates, radius));
        for (int i = 0; i < Math.min(LISTED, found.size()); i++) {
            RuinFacet.Ruin ruin = found.get(i);
            out.append(Console.NEW_LINE).append(String.format(Locale.ROOT,
                    "  %-9s face %d  at %d %d %d  (%.0f blocks)",
                    ruin.culture().name().toLowerCase(Locale.ROOT), ruin.site().face(),
                    ruin.site().x(), ruin.floorY() + 2, ruin.site().z() + 3,
                    Math.sqrt(distance2(ruin, px, pz))));
        }
        return out.toString();
    }

    private static long distance2(RuinFacet.Ruin ruin, int x, int z) {
        long dx = ruin.site().x() - x;
        long dz = ruin.site().z() - z;
        return dx * dx + dz * dz;
    }
}
