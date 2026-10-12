// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.joml.Vector3f;
import org.terasology.herocubeworlds.generator.CubeWorldGenerator;
import org.terasology.herocubeworlds.generator.RuinLattice;
import org.terasology.herocubeworlds.generator.facetProviders.CubeRuinProvider;
import org.terasology.herocubeworlds.generator.facets.RuinFacet;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.logic.console.Console;
import org.terasology.engine.logic.console.commandSystem.annotations.Command;
import org.terasology.engine.logic.console.commandSystem.annotations.CommandParam;
import org.terasology.engine.logic.permission.PermissionManager;
import org.terasology.engine.logic.players.LocalPlayer;
import org.terasology.engine.registry.In;
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
 * is the generator's own, taken once per site from the elevation and the biome over the footprint and
 * remembered, so asking every candidate within two kilometres costs a few dozen small surveys.
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
        CubeRuinProvider ruins = generator.getRuinProvider();
        if (ruins.lattice() == null) {
            return "the generator has not been seeded yet";
        }
        int radius = radiusParam != null ? radiusParam : DEFAULT_RADIUS;
        if (radius < 1 || radius > MAX_RADIUS) {
            return "the radius runs from 1 to " + MAX_RADIUS + " blocks";
        }

        Vector3f eye = localPlayer.getPosition(new Vector3f());
        int px = Math.round(eye.x);
        int pz = Math.round(eye.z);

        int candidates = 0;
        List<RuinFacet.Ruin> found = new ArrayList<>();
        for (RuinLattice.Site site : ruins.lattice().sitesNear(px, pz, px, pz, radius)) {
            if (distance2(site, px, pz) > (long) radius * radius) {
                continue;
            }
            candidates++;
            ruins.judge(site).ifPresent(found::add);
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
        return distance2(ruin.site(), x, z);
    }

    private static long distance2(RuinLattice.Site site, int x, int z) {
        long dx = site.x() - x;
        long dz = site.z() - z;
        return dx * dx + dz * dz;
    }
}
