// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.joml.Vector3ic;
import org.terasology.cubeworlds.generator.CubeGeometry;
import org.terasology.engine.context.Context;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.registry.In;
import org.terasology.engine.world.block.entity.placement.PlaceBlocks;
import org.terasology.gestalt.entitysystem.event.ReceiveEvent;

/**
 * Refuses to place a block outside the world, top or bottom.
 * <p>
 * The generator ends the world at {@link CubeGeometry#floorY()} and {@link CubeGeometry#ceilingY()},
 * but that only decides what is generated. Nothing stopped a player from stacking dirt past the top
 * of the world, and the chunks up there would have been loaded and meshed to hold it.
 * <p>
 * One receiver covers every placement in the running game. {@code PlaceBlocks} is what the player's
 * block item sends and also what the fluid module sends; explosives never place anything, they only
 * remove. Consuming it stops the event before {@code BlockPlacingSystem}, which listens at trivial
 * priority, and the item is not lost either: {@code BlockItemSystem} consumes the activation before
 * the inventory gets to decrement the stack.
 * <p>
 * <b>Known leak.</b> The placement sound and the blocks-placed statistic both sit outside the
 * consumed check in {@code BlockItemSystem}, so a refused placement still makes a noise and still
 * counts. Fixing that means changing the engine.
 * <p>
 * The bounds come from the geometry in the context rather than from constants of our own, so that
 * the rule and the generator cannot disagree. A world built by some other generator has no geometry
 * there, and then this system does nothing at all — which is right: these are the cube world's
 * bounds, not a rule of the game.
 */
@RegisterSystem(RegisterMode.AUTHORITY)
public class VerticalBuildLimitSystem extends BaseComponentSystem {

    @In
    private Context context;

    private CubeGeometry geometry;

    @ReceiveEvent
    public void refuseToBuildOutsideTheWorld(PlaceBlocks event, EntityRef world) {
        CubeGeometry bounds = bounds();
        if (bounds == null) {
            return;
        }
        for (Vector3ic position : event.getBlocks().keySet()) {
            if (position.y() < bounds.floorY() || position.y() > bounds.ceilingY()) {
                event.consume();
                return;
            }
        }
    }

    /**
     * Looked up on use and never cached as absent: the generator publishes the geometry while the
     * world loads, which is not guaranteed to be before this system is wired up. Caching a null
     * would switch the limit off for the whole session, and silently.
     */
    private CubeGeometry bounds() {
        if (geometry == null && context != null) {
            geometry = context.get(CubeGeometry.class);
        }
        return geometry;
    }
}
