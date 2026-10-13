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
import org.terasology.engine.core.Time;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.entitySystem.systems.UpdateSubscriberSystem;
import org.terasology.engine.logic.characters.CharacterComponent;
import org.terasology.engine.logic.characters.CharacterTeleportEvent;
import org.terasology.engine.logic.console.commandSystem.annotations.Command;
import org.terasology.engine.logic.console.commandSystem.annotations.CommandParam;
import org.terasology.engine.logic.console.commandSystem.annotations.Sender;
import org.terasology.engine.logic.location.LocationComponent;
import org.terasology.engine.logic.notifications.NotificationMessageEvent;
import org.terasology.engine.logic.permission.PermissionManager;
import org.terasology.engine.network.ClientComponent;
import org.terasology.engine.registry.In;
import org.terasology.engine.world.RelevanceRegionComponent;
import org.terasology.engine.world.WorldProvider;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockManager;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.engine.world.generator.WorldGenerator;
import org.terasology.gestalt.entitysystem.event.ReceiveEvent;
import org.terasology.herocubeworlds.generator.CubeGeometry;
import org.terasology.herocubeworlds.generator.CubeWorldGenerator;
import org.terasology.herocubeworlds.generator.RuinLattice;
import org.terasology.herocubeworlds.generator.RuinShape;
import org.terasology.herocubeworlds.generator.facetProviders.CubeRuinProvider;
import org.terasology.herocubeworlds.generator.facets.RuinFacet;

import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;

/**
 * Opens, enters, leaves and wipes the pockets of the past.
 * <p>
 * <b>The way in is a window at a ruin.</b> At a ruin's anchor stone the hero's innate power (D12, the
 * {@link AnchorButton}) opens a window across the axis of the gate; one walks through it into the
 * pocket, and through the window at the back of the room out again — nothing is teleported on a key
 * press but the way out of last resort. The window stays open one to two hours (D16), then closes of
 * itself and throws out whoever is still inside. Only the characters at the anchor stone when it
 * opened may cross it (D17), and closing wipes the pocket (D9).
 * <p>
 * The console verbs of lot 0 stay, as the bench: a pocket with no ruin, no window and no timer.
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

    /** Chunks kept loaded around a ruin whose window is open, so the window can always be taken down. */
    private static final Vector3ic KEEPER_RELEVANCE = new Vector3i(1, 1, 1);

    /** How far from the anchor stone the power answers, in blocks, across and up or down. */
    private static final int ANCHOR_REACH = 6;

    /** How far from the anchor stone a character must stand to be one of the group that goes in. */
    private static final int GROUP_REACH = 12;

    /** How long before closing the group is warned, in milliseconds. */
    private static final long WARNING = 5 * 60 * 1000L;

    /** How often a character turned away at a window is told why, in milliseconds. */
    private static final long REFUSAL_COOLDOWN = 3000L;

    @In
    private Time time;

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

    /** When each character turned away at a window was last told why. Not saved: a courtesy, not a state. */
    private final Map<EntityRef, Long> refused = new HashMap<>();

    @Override
    public void update(float delta) {
        if (geometry() == null) {
            return;
        }
        long now = time.getGameTimeInMs();
        for (EntityRef pocket : entityManager.getEntitiesWith(PocketComponent.class)) {
            PocketComponent state = pocket.getComponent(PocketComponent.class);
            if (state.closing) {
                finishClosing(pocket, state);
                continue;
            }
            if (!state.built) {
                build(pocket, state);
                continue;
            }
            if (state.closesAt > 0) {
                if (now >= state.closesAt) {
                    close(pocket, state, "La fenêtre se referme : le passé vous rejette.");
                    continue;
                }
                if (!state.warned && now >= state.closesAt - WARNING) {
                    state.warned = true;
                    pocket.saveComponent(state);
                    tellGroup(state, "La fenêtre se refermera dans cinq minutes.");
                }
            }
        }
        crossings(now);
    }

    /** Lays the room down once every chunk of its box answers, and lets in whoever was waiting for it. */
    private void build(EntityRef pocket, PocketComponent state) {
        Vector3i centre = centre(state.plot);
        if (!loaded(PocketRoom.box(centre))) {
            return;
        }
        Map<Vector3ic, Block> blocks = lay(centre, true);
        worldProvider.setBlocks(blocks);
        state.built = true;
        EntityRef waiting = state.waiting;
        state.waiting = EntityRef.NULL;
        pocket.saveComponent(state);
        logger.info("Pocket {} laid down at {}: {} blocks", state.plot, centre, blocks.size());
        if (waiting.exists()) {
            enter(waiting, state.plot, null);
        }
    }

    /**
     * Who walks through a window this tick. A character stands in a window when its body's block is one
     * of the window's: the window is a slab one block deep and a walker covers a fraction of a block a
     * tick, so no step jumps it.
     */
    private void crossings(long now) {
        for (EntityRef client : entityManager.getEntitiesWith(ClientComponent.class)) {
            EntityRef character = character(client);
            LocationComponent location = character.getComponent(LocationComponent.class);
            if (location == null) {
                continue;
            }
            Vector3f at = location.getWorldPosition(new Vector3f());
            int x = Math.round(at.x);
            int y = Math.round(at.y);
            int z = Math.round(at.z);
            PocketVisitorComponent visit = character.getComponent(PocketVisitorComponent.class);
            if (visit != null) {
                Vector3i centre = centre(visit.plot);
                // Two cells of a body: a character's position is the middle of it, its feet a block lower.
                if (PocketRoom.isExit(x - centre.x(), y - centre.y(), z - centre.z())
                        || PocketRoom.isExit(x - centre.x(), y - 1 - centre.y(), z - centre.z())) {
                    expel(character, visit);
                }
                continue;
            }
            for (EntityRef pocket : entityManager.getEntitiesWith(PocketComponent.class)) {
                PocketComponent state = pocket.getComponent(PocketComponent.class);
                if (!state.fromRuin || !state.built || state.closing) {
                    continue;
                }
                RuinWindow window = window(state);
                if (!window.contains(x, y, z) && !window.contains(x, y - 1, z)) {
                    continue;
                }
                if (state.members.contains(character)) {
                    enter(character, state.plot, window.outside());
                } else {
                    Long last = refused.get(character);
                    if (last == null || now - last > REFUSAL_COOLDOWN) {
                        refused.put(character, now);
                        tell(character, "Cette fenêtre s'est ouverte sans vous : on ne rejoint pas un passé en cours.");
                    }
                }
                break;
            }
        }
    }

    /**
     * The anchor power. In a pocket, it tears the character out (the key of D29's lever); at a ruin's
     * anchor stone, it opens the window onto that ruin's past.
     */
    @ReceiveEvent(components = CharacterComponent.class)
    public void onAnchorRequest(AnchorRequest request, EntityRef character) {
        tell(character, useAnchor(character));
    }

    @Command(shortDescription = "Use le pouvoir d'ancre, comme la touche P",
            helpText = "À la pierre d'ancre d'une ruine : ouvre la fenêtre sur son passé. Dans une poche : vous en "
                    + "arrache.",
            runOnServer = true,
            requiredPermission = PermissionManager.CHEAT_PERMISSION)
    public String ancre(@Sender EntityRef client) {
        return useAnchor(character(client));
    }

    @Command(shortDescription = "Avance l'échéance de la fenêtre où vous êtes, pour l'éprouver",
            helpText = "La fenêtre de la poche où vous êtes, ou de la ruine la plus proche, se refermera dans le "
                    + "nombre de secondes donné.",
            runOnServer = true,
            requiredPermission = PermissionManager.CHEAT_PERMISSION)
    public String ancreEcheance(@Sender EntityRef client, @CommandParam("secondes") int seconds) {
        EntityRef character = character(client);
        EntityRef pocket = pocketOf(character);
        if (!pocket.exists()) {
            return "Aucune fenêtre ouverte.";
        }
        PocketComponent state = pocket.getComponent(PocketComponent.class);
        state.closesAt = time.getGameTimeInMs() + seconds * 1000L;
        state.warned = seconds * 1000L <= WARNING;
        pocket.saveComponent(state);
        return "La fenêtre " + state.plot + " se refermera dans " + seconds + " s.";
    }

    private String useAnchor(EntityRef character) {
        if (geometry() == null) {
            return "Le pouvoir d'ancre ne répond que sur un monde-cube.";
        }
        LocationComponent location = character.getComponent(LocationComponent.class);
        if (location == null) {
            return "Aucun personnage.";
        }
        PocketVisitorComponent visit = character.getComponent(PocketVisitorComponent.class);
        if (visit != null) {
            expel(character, visit);
            return "Vous vous arrachez au passé.";
        }
        Vector3f at = location.getWorldPosition(new Vector3f());
        Optional<RuinFacet.Ruin> found = ruinNear(at);
        if (found.isEmpty()) {
            return "Aucune pierre d'ancre à portée : le pouvoir ne trouve rien à quoi s'accrocher.";
        }
        RuinFacet.Ruin ruin = found.get();
        for (EntityRef pocket : entityManager.getEntitiesWith(PocketComponent.class)) {
            PocketComponent state = pocket.getComponent(PocketComponent.class);
            if (state.fromRuin && state.ruinX == ruin.site().x() && state.ruinZ == ruin.site().z()) {
                return "La fenêtre de cette ruine est déjà ouverte.";
            }
        }
        int plot = freePlot();
        if (plot < 0) {
            return "Le passé est saturé : les " + plots.count() + " poches sont toutes ouvertes.";
        }
        return open(ruin, plot);
    }

    /** Opens the window of a ruin onto a fresh pocket, for the characters standing at its anchor stone. */
    private String open(RuinFacet.Ruin ruin, int plot) {
        PocketComponent state = new PocketComponent();
        state.plot = plot;
        state.fromRuin = true;
        state.ruinX = ruin.site().x();
        state.ruinZ = ruin.site().z();
        state.floorY = ruin.floorY();
        state.gateSide = RuinShape.gateSide(ruin.site().seed());
        int minutes = RuinWindow.minutes(ruin.site().seed());
        state.closesAt = time.getGameTimeInMs() + minutes * 60_000L;
        RuinWindow window = window(state);
        Vector3f anchor = window.anchor();
        for (EntityRef client : entityManager.getEntitiesWith(ClientComponent.class)) {
            EntityRef member = character(client);
            LocationComponent location = member.getComponent(LocationComponent.class);
            if (location != null && location.getWorldPosition(new Vector3f()).distance(anchor) <= GROUP_REACH) {
                state.members.add(member);
            }
        }
        state.keeper = entityManager.create(new LocationComponent(anchor), new RelevanceRegionComponent(KEEPER_RELEVANCE));
        entityManager.create(state, new LocationComponent(new Vector3f(centre(plot))),
                new RelevanceRegionComponent(RELEVANCE));

        Block pane = blockManager.getBlock("HeroDungeons:Fenetre");
        Block air = blockManager.getBlock(BlockManager.AIR_ID);
        Map<Vector3ic, Block> blocks = new HashMap<>();
        for (Vector3i cell : window.cells()) {
            if (worldProvider.getBlock(cell) == air) {
                blocks.put(cell, pane);
            }
        }
        worldProvider.setBlocks(blocks);
        logger.info("Window of the ruin at ({}, {}) opened onto pocket {} for {} min, {} member(s)",
                state.ruinX, state.ruinZ, plot, minutes, state.members.size());
        return String.format("La pierre d'ancre répond : une fenêtre s'ouvre sur le passé de cette ruine. "
                + "Elle se refermera dans %d minutes.", minutes);
    }

    /** Empties and wipes a pocket, then takes its window down; the window waits if its ruin is not loaded. */
    private void close(EntityRef pocket, PocketComponent state, String message) {
        for (EntityRef visitor : entityManager.getEntitiesWith(PocketVisitorComponent.class)) {
            PocketVisitorComponent visit = visitor.getComponent(PocketVisitorComponent.class);
            if (visit.plot == state.plot) {
                expel(visitor, visit);
                tell(visitor, message);
            }
        }
        if (state.built) {
            worldProvider.setBlocks(lay(centre(state.plot), false));
        }
        state.closing = true;
        pocket.saveComponent(state);
        finishClosing(pocket, state);
    }

    private void finishClosing(EntityRef pocket, PocketComponent state) {
        if (state.fromRuin) {
            RuinWindow window = window(state);
            List<Vector3i> cells = window.cells();
            for (Vector3i cell : cells) {
                if (!worldProvider.isBlockRelevant(cell)) {
                    return;
                }
            }
            Block pane = blockManager.getBlock("HeroDungeons:Fenetre");
            Block air = blockManager.getBlock(BlockManager.AIR_ID);
            Map<Vector3ic, Block> blocks = new HashMap<>();
            for (Vector3i cell : cells) {
                if (worldProvider.getBlock(cell) == pane) {
                    blocks.put(cell, air);
                }
            }
            worldProvider.setBlocks(blocks);
            state.keeper.destroy();
        }
        pocket.destroy();
    }

    private Optional<RuinFacet.Ruin> ruinNear(Vector3f at) {
        if (!(context.get(WorldGenerator.class) instanceof CubeWorldGenerator cube)) {
            return Optional.empty();
        }
        CubeRuinProvider ruins = cube.getRuinProvider();
        if (ruins.lattice() == null) {
            return Optional.empty();
        }
        int x = Math.round(at.x);
        int z = Math.round(at.z);
        for (RuinLattice.Site site : ruins.lattice().sitesNear(x, z, x, z, ANCHOR_REACH)) {
            Optional<RuinFacet.Ruin> ruin = ruins.judge(site);
            if (ruin.isPresent() && Math.abs(at.y - ruin.get().floorY()) <= ANCHOR_REACH) {
                return ruin;
            }
        }
        return Optional.empty();
    }

    private static RuinWindow window(PocketComponent state) {
        return new RuinWindow(state.ruinX, state.ruinZ, state.floorY, state.gateSide);
    }

    private void tellGroup(PocketComponent state, String message) {
        for (EntityRef member : state.members) {
            tell(member, message);
        }
    }

    private static void tell(EntityRef character, String message) {
        CharacterComponent body = character.getComponent(CharacterComponent.class);
        if (body != null && body.controller.exists()) {
            body.controller.send(new NotificationMessageEvent(message, body.controller));
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
        int inside = 0;
        for (EntityRef visitor : entityManager.getEntitiesWith(PocketVisitorComponent.class)) {
            if (visitor.getComponent(PocketVisitorComponent.class).plot == state.plot) {
                inside++;
            }
        }
        close(pocket, state, "La poche se referme.");
        return String.format("Poche %d refermée : %d expulsé(s), effacée.", state.plot, inside);
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

    /**
     * @param returnPosition where the character comes back out, or null for where it stands now: a window
     *         gives the spot on its outer side, so that coming back does not land in the window again
     */
    private void enter(EntityRef character, int plot, Vector3f returnPosition) {
        LocationComponent location = character.getComponent(LocationComponent.class);
        if (location == null) {
            return;
        }
        PocketVisitorComponent visit = new PocketVisitorComponent();
        visit.plot = plot;
        visit.returnPosition = returnPosition != null ? returnPosition : location.getWorldPosition(new Vector3f());
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
        Block pane = blockManager.getBlock("HeroDungeons:Fenetre");
        Map<Vector3ic, Block> blocks = new HashMap<>();
        for (Vector3ic pos : PocketRoom.box(centre)) {
            PocketRoom.Kind kind = build
                    ? PocketRoom.at(pos.x() - centre.x(), pos.y() - centre.y(), pos.z() - centre.z())
                    : PocketRoom.Kind.AIR;
            Block block = switch (kind) {
                case FLOOR -> floor;
                case WALL -> wall;
                case PILLAR -> pillar;
                case WINDOW -> pane;
                case AIR -> air;
            };
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
