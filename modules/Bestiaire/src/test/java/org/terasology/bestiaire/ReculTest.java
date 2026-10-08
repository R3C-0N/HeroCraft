// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.bestiaire;

import org.joml.Quaternionf;
import org.joml.Vector3f;
import org.joml.Vector3i;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Tag;
import org.junit.jupiter.api.Test;
import org.terasology.engine.core.ComponentSystemManager;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.systems.UpdateSubscriberSystem;
import org.terasology.engine.integrationenvironment.MainLoop;
import org.terasology.engine.integrationenvironment.jupiter.IntegrationEnvironment;
import org.terasology.engine.logic.characters.CharacterMovementComponent;
import org.terasology.engine.logic.location.LocationComponent;
import org.terasology.engine.registry.In;
import org.terasology.engine.world.WorldProvider;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockManager;
import org.terasology.engine.world.block.BlockRegion;
import org.terasology.module.health.events.OnDamagedEvent;

import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The shove, frame by frame: it slows down rather than stopping dead, it rears, it stands back up, and a wall
 * stops it. And a bite sends the character backwards.
 */
@Tag("MteTest")
@IntegrationEnvironment(dependencies = {"Bestiaire"})
public class ReculTest {

    private static final float DELTA = 1f / 60f;

    @In
    private EntityManager entityManager;
    @In
    private WorldProvider worldProvider;
    @In
    private BlockManager blockManager;
    @In
    private ComponentSystemManager systemes;
    @In
    private MainLoop mainLoop;

    private Block pierre;

    @BeforeEach
    public void sol() {
        mainLoop.runUntil(mainLoop.makeBlocksRelevant(new BlockRegion(-16, -4, -16).union(16, 8, 16)));
        pierre = blockManager.getBlock("CoreAssets:Stone");
        for (int x = -16; x <= 16; x++) {
            for (int z = -16; z <= 16; z++) {
                worldProvider.setBlock(new Vector3i(x, 0, z), pierre);
            }
        }
    }

    @Test
    public void laBeteFreineSeCabreEtSeRedresse() {
        EntityRef loup = poser("Bestiaire:mouflon", new Vector3f(0, 2, 0));
        Quaternionf avant = rotation(loup);
        float x0 = position(loup).x;

        frapper(loup, new Vector3f(-2, 1, 0));
        assertTrue(loup.hasComponent(ReculComponent.class));

        List<Float> pas = new ArrayList<>();
        float penchePlus = 0f;
        float xPrecedent = x0;
        int images = 0;
        while (loup.hasComponent(ReculComponent.class) && images++ < 200) {
            image();
            float x = position(loup).x;
            pas.add(x - xPrecedent);
            xPrecedent = x;
            penchePlus = Math.max(penchePlus, rotation(loup).angle());
        }

        assertFalse(loup.hasComponent(ReculComponent.class), "the shove never ended");
        float distance = position(loup).x - x0;
        assertTrue(distance > 0.4f && distance < 2f, "thrown " + distance + " blocks");
        // Each frame moves less than the one before: a slide that slows, not one that stops dead.
        for (int i = 2; i < pas.size(); i++) {
            assertTrue(pas.get(i) <= pas.get(i - 1) + 1e-4f, "frame " + i + " moved more than " + (i - 1) + ": " + pas);
        }
        assertTrue(pas.get(0) > 0.05f, "the first frame barely moved: " + pas);
        assertTrue(penchePlus > Math.toRadians(5), "never reared: " + Math.toDegrees(penchePlus));
        assertTrue(rotation(loup).equals(avant, 1e-5f), "not upright again");
        float pieds = position(loup).y - Stride.demiHauteur(loup);
        assertEquals(0.5f, pieds, 1e-3f, "not back on the floor");
    }

    @Test
    public void unMurArreteLeRecul() {
        for (int y = 1; y <= 3; y++) {
            for (int z = -2; z <= 2; z++) {
                worldProvider.setBlock(new Vector3i(2, y, z), pierre);
            }
        }
        EntityRef bete = poser("Bestiaire:mouflon", new Vector3f(0.5f, 2, 0));
        frapper(bete, new Vector3f(-2, 1, 0));
        for (int i = 0; i < 120 && bete.hasComponent(ReculComponent.class); i++) {
            image();
        }
        assertFalse(bete.hasComponent(ReculComponent.class));
        assertTrue(position(bete).x < 1.5f, "went through the wall: " + position(bete).x);
    }

    @Test
    public void uneMorsureRepousseLePersonnage() {
        EntityRef personnage = entityManager.create("engine:player", new Vector3f(0, 1.5f, 0));
        for (int i = 0; i < 30; i++) {
            image();
        }
        EntityRef loup = poser("Bestiaire:loup", new Vector3f(-1.5f, 2, 0));
        personnage.send(new OnDamagedEvent(6, null, loup));
        // Le personnage n'avance que quand son client lui envoie une entree, et il n'y a pas de client ici : on
        // lit donc la vitesse que la poussee lui laisse, celle que son prochain pas integrera.
        Vector3f vitesse = personnage.getComponent(CharacterMovementComponent.class).getVelocity();
        assertTrue(vitesse.x > 2f, "not pushed away from the wolf: " + vitesse);
        assertTrue(vitesse.y > 0f, "not lifted off the ground: " + vitesse);
        assertEquals(0f, vitesse.z, 1e-3f, "pushed sideways: " + vitesse);
    }

    private EntityRef poser(String prefab, Vector3f ou) {
        EntityRef bete = entityManager.create(prefab, ou);
        // On la laisse se poser, puis on lui retire ses jambes : seul le recul doit la deplacer ici.
        for (int i = 0; i < 30; i++) {
            image();
        }
        bete.removeComponent(WanderComponent.class);
        bete.removeComponent(PredatorComponent.class);
        return bete;
    }

    private void frapper(EntityRef bete, Vector3f depuis) {
        EntityRef frappeur = entityManager.create(new LocationComponent(depuis));
        bete.send(new OnDamagedEvent(4, null, frappeur));
    }

    private void image() {
        for (UpdateSubscriberSystem systeme : systemes.iterateUpdateSubscribers()) {
            systeme.update(DELTA);
        }
    }

    private static Vector3f position(EntityRef e) {
        return e.getComponent(LocationComponent.class).getWorldPosition(new Vector3f());
    }

    private static Quaternionf rotation(EntityRef e) {
        return e.getComponent(LocationComponent.class).getWorldRotation(new Quaternionf());
    }
}
