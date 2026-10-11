// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.joml.Vector2ic;
import org.joml.Vector3f;
import org.joml.Vector3i;
import org.joml.Vector3ic;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.terasology.engine.context.Context;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.entitySystem.systems.UpdateSubscriberSystem;
import org.terasology.engine.logic.characters.CharacterTeleportEvent;
import org.terasology.engine.logic.console.commandSystem.annotations.Command;
import org.terasology.engine.logic.console.commandSystem.annotations.CommandParam;
import org.terasology.engine.logic.console.commandSystem.annotations.Sender;
import org.terasology.engine.logic.location.LocationComponent;
import org.terasology.engine.logic.permission.PermissionManager;
import org.terasology.engine.network.ClientComponent;
import org.terasology.engine.registry.In;
import org.terasology.engine.world.RelevanceRegionComponent;
import org.terasology.engine.world.WorldProvider;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockManager;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generator.WorldGenerator;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.CubeWorldGenerator;

import java.util.HashMap;
import java.util.Map;

/**
 * Opens, enters, leaves and wipes the pockets of the past. The bench of lot 0: no ruin, no window,
 * no timer yet, only the four console verbs that prove a pocket can be held apart from the planet.
 * <p>
 * <b>Opening is not instant, and that is the engine's doing.</b> The dead slot is a kilometre from
 * anything loaded, so the pocket's entity carries a relevance region and the room is laid down only
 * once every chunk of its box answers. A character sent there before would fall through a floor
 * that does not exist yet.
 * <p>
 * <b>Wiping walks the same box as building.</b> {@link PocketRoom#box} covers everything the room
 * may write and the headroom above it, so whatever the players put down inside goes with it, and the
 * next pocket opened on that plot starts from nothing (D9).
 */
@RegisterSystem(RegisterMode.AUTHORITY)
public class PocketSystem extends BaseComponentSystem implements UpdateSubscriberSystem {

    private static final Logger logger = LoggerFactory.getLogger(PocketSystem.class);

    /** Chunks kept loaded around a pocket's centre: its room, with a chunk to spare on every side. */
    private static final Vector3ic RELEVANCE = new Vector3i(5, 3, 5);

    @In
    private Context context;

    @In
    private EntityManager entityManager;

    @In
    private WorldProvider worldProvider;

    @In
    private BlockManager blockManager;

    private PocketPlots plots;
    private CubeGeometry geometry;

    @Override
    public void update(float delta) {
        for (EntityRef pocket : entityManager.getEntitiesWith(PocketComponent.class)) {
            PocketComponent state = pocket.getComponent(PocketComponent.class);
            if (state.built || geometry() == null) {
                continue;
            }
            Vector3i centre = centre(state.plot);
            if (!loaded(PocketRoom.box(centre))) {
                continue;
            }
            Map<Vector3ic, Block> blocks = lay(centre, true);
            worldProvider.setBlocks(blocks);
            state.built = true;
            EntityRef waiting = state.waiting;
            state.waiting = EntityRef.NULL;
            pocket.saveComponent(state);
            logger.info("Pocket {} laid down at {}: {} blocks", state.plot, centre, blocks.size());
            if (waiting.exists()) {
                enter(waiting, state.plot);
            }
        }
    }

    @Command(shortDescription = "Ouvre une poche du passé et vous y envoie",
            helpText = "Banc du lot 0 : une salle plate posée dans une case morte du patron, le temps que ses "
                    + "chunks se chargent. « poche » seule dit ce qui est ouvert.",
            runOnServer = true,
            requiredPermission = PermissionManager.CHEAT_PERMISSION)
    public String pocheOuvrir(@Sender EntityRef client) {
        if (geometry() == null) {
            return "Pas de poche hors d'un monde-cube.";
        }
        EntityRef character = character(client);
        if (!character.exists()) {
            return "Aucun personnage.";
        }
        if (character.hasComponent(PocketVisitorComponent.class)) {
            return "Vous êtes déjà dans une poche.";
        }
        int plot = freePlot();
        if (plot < 0) {
            return "Les " + plots.count() + " poches sont toutes ouvertes.";
        }
        Vector3i centre = centre(plot);
        PocketComponent state = new PocketComponent();
        state.plot = plot;
        state.waiting = character;
        LocationComponent location = new LocationComponent(new Vector3f(centre));
        entityManager.create(state, location, new RelevanceRegionComponent(RELEVANCE));
        return String.format("Poche %d ouverte en (%d, %d, %d) ; vous y entrerez quand elle sera chargée.",
                plot, centre.x(), centre.y(), centre.z());
    }

    @Command(shortDescription = "Vous ramène de la poche, là où vous étiez",
            runOnServer = true,
            requiredPermission = PermissionManager.CHEAT_PERMISSION)
    public String pocheSortir(@Sender EntityRef client) {
        EntityRef character = character(client);
        PocketVisitorComponent visit = character.getComponent(PocketVisitorComponent.class);
        if (visit == null) {
            return "Vous n'êtes dans aucune poche.";
        }
        expel(character, visit);
        return "Retour en " + visit.returnPosition + ".";
    }

    @Command(shortDescription = "Referme une poche : expulse ses occupants et l'efface",
            helpText = "Sans numéro : la poche où vous êtes, ou la dernière ouverte. Tout ce qui a été posé "
                    + "dedans disparaît ; une poche rouverte sur la même parcelle repart à neuf.",
            runOnServer = true,
            requiredPermission = PermissionManager.CHEAT_PERMISSION)
    public String pocheFermer(@Sender EntityRef client,
                              @CommandParam(value = "numéro", required = false) Integer numero) {
        EntityRef pocket = numero != null ? pocketOn(numero) : pocketOf(character(client));
        if (!pocket.exists()) {
            return "Aucune poche à refermer.";
        }
        PocketComponent state = pocket.getComponent(PocketComponent.class);
        int expelled = 0;
        for (EntityRef visitor : entityManager.getEntitiesWith(PocketVisitorComponent.class)) {
            PocketVisitorComponent visit = visitor.getComponent(PocketVisitorComponent.class);
            if (visit.plot == state.plot) {
                expel(visitor, visit);
                expelled++;
            }
        }
        int wiped = 0;
        if (state.built) {
            Map<Vector3ic, Block> air = lay(centre(state.plot), false);
            wiped = air.size();
            worldProvider.setBlocks(air);
        }
        pocket.destroy();
        return String.format("Poche %d refermée : %d expulsé(s), %d blocs effacés.", state.plot, expelled, wiped);
    }

    @Command(shortDescription = "Dit quelles poches sont ouvertes, et qui s'y trouve",
            runOnServer = true,
            requiredPermission = PermissionManager.CHEAT_PERMISSION)
    public String poche() {
        if (geometry() == null) {
            return "Pas de poche hors d'un monde-cube.";
        }
        StringBuilder text = new StringBuilder();
        int open = 0;
        for (EntityRef pocket : entityManager.getEntitiesWith(PocketComponent.class)) {
            PocketComponent state = pocket.getComponent(PocketComponent.class);
            Vector3i centre = centre(state.plot);
            int inside = 0;
            for (EntityRef visitor : entityManager.getEntitiesWith(PocketVisitorComponent.class)) {
                if (visitor.getComponent(PocketVisitorComponent.class).plot == state.plot) {
                    inside++;
                }
            }
            text.append(String.format("  %d en (%d, %d, %d) : %s, %d dedans%n", state.plot,
                    centre.x(), centre.y(), centre.z(), state.built ? "posée" : "en chargement", inside));
            open++;
        }
        return String.format("Poches ouvertes : %d sur %d%n", open, plots.count()) + text.toString().stripTrailing();
    }

    private void enter(EntityRef character, int plot) {
        LocationComponent location = character.getComponent(LocationComponent.class);
        if (location == null) {
            return;
        }
        PocketVisitorComponent visit = new PocketVisitorComponent();
        visit.plot = plot;
        visit.returnPosition = location.getWorldPosition(new Vector3f());
        character.addComponent(visit);
        character.send(new CharacterTeleportEvent(new Vector3f(PocketRoom.arrival(centre(plot)))));
    }

    private void expel(EntityRef character, PocketVisitorComponent visit) {
        character.removeComponent(PocketVisitorComponent.class);
        character.send(new CharacterTeleportEvent(new Vector3f(visit.returnPosition)));
    }

    /** The room's blocks around a centre, or air over the same box when {@code build} is false. */
    private Map<Vector3ic, Block> lay(Vector3i centre, boolean build) {
        Block air = blockManager.getBlock(BlockManager.AIR_ID);
        Block floor = blockManager.getBlock("CoreAssets:Stone");
        Block wall = blockManager.getBlock("CoreAssets:CobbleStone");
        Block pillar = blockManager.getBlock("CoreAssets:Brick");
        Map<Vector3ic, Block> blocks = new HashMap<>();
        for (Vector3ic pos : PocketRoom.box(centre)) {
            Block block = air;
            if (build) {
                switch (PocketRoom.at(pos.x() - centre.x(), pos.y() - centre.y(), pos.z() - centre.z())) {
                    case FLOOR -> block = floor;
                    case WALL -> block = wall;
                    case PILLAR -> block = pillar;
                    default -> block = air;
                }
            }
            blocks.put(new Vector3i(pos), block);
        }
        return blocks;
    }

    private boolean loaded(BlockRegion box) {
        return worldProvider.isBlockRelevant(new Vector3i(box.minX(), box.minY(), box.minZ()))
                && worldProvider.isBlockRelevant(new Vector3i(box.maxX(), box.maxY(), box.maxZ()))
                && worldProvider.isBlockRelevant(new Vector3i(box.minX(), box.minY(), box.maxZ()))
                && worldProvider.isBlockRelevant(new Vector3i(box.maxX(), box.maxY(), box.minZ()));
    }

    private Vector3i centre(int plot) {
        Vector2ic column = plots.centre(plot);
        return new Vector3i(column.x(), geometry.seaLevel(), column.y());
    }

    private int freePlot() {
        boolean[] taken = new boolean[plots.count()];
        for (EntityRef pocket : entityManager.getEntitiesWith(PocketComponent.class)) {
            int plot = pocket.getComponent(PocketComponent.class).plot;
            if (plot >= 0 && plot < taken.length) {
                taken[plot] = true;
            }
        }
        for (int plot = 0; plot < taken.length; plot++) {
            if (!taken[plot]) {
                return plot;
            }
        }
        return -1;
    }

    private EntityRef pocketOn(int plot) {
        for (EntityRef pocket : entityManager.getEntitiesWith(PocketComponent.class)) {
            if (pocket.getComponent(PocketComponent.class).plot == plot) {
                return pocket;
            }
        }
        return EntityRef.NULL;
    }

    /** The pocket the character stands in, else the last one opened. */
    private EntityRef pocketOf(EntityRef character) {
        PocketVisitorComponent visit = character.getComponent(PocketVisitorComponent.class);
        if (visit != null) {
            return pocketOn(visit.plot);
        }
        EntityRef last = EntityRef.NULL;
        for (EntityRef pocket : entityManager.getEntitiesWith(PocketComponent.class)) {
            last = pocket;
        }
        return last;
    }

    private static EntityRef character(EntityRef client) {
        ClientComponent component = client.getComponent(ClientComponent.class);
        return component == null ? EntityRef.NULL : component.character;
    }

    /** The cube's geometry, or null on a world that is not a cube and so has no dead slots. */
    private CubeGeometry geometry() {
        if (geometry == null) {
            geometry = context.get(CubeGeometry.class);
            if (geometry == null && context.get(WorldGenerator.class) instanceof CubeWorldGenerator cube) {
                geometry = cube.getGeometry();
            }
            if (geometry != null) {
                plots = new PocketPlots(geometry.faceBlocks());
            }
        }
        return geometry;
    }
}
