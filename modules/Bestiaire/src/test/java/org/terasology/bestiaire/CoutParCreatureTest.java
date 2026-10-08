// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.bestiaire;

import org.joml.Quaternionf;
import org.joml.Vector3f;
import org.joml.Vector3i;
import org.junit.jupiter.api.Tag;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.condition.EnabledIfEnvironmentVariable;
import org.terasology.engine.core.ComponentSystemManager;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.event.internal.EventSystem;
import org.terasology.engine.entitySystem.systems.UpdateSubscriberSystem;
import org.terasology.engine.integrationenvironment.MainLoop;
import org.terasology.engine.integrationenvironment.jupiter.IntegrationEnvironment;
import org.terasology.engine.logic.characters.AliveCharacterComponent;
import org.terasology.engine.logic.characters.CharacterComponent;
import org.terasology.engine.logic.location.LocationComponent;
import org.terasology.engine.registry.In;
import org.terasology.engine.rendering.logic.SkeletalMeshComponent;
import org.terasology.engine.rendering.logic.SkeletonRenderer;
import org.terasology.engine.world.WorldProvider;
import org.terasology.engine.world.block.Block;
import org.terasology.engine.world.block.BlockManager;
import org.terasology.engine.world.block.BlockRegion;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * What one creature costs per frame, system by system, without a screen.
 * <p>
 * A bench, not a test: it asserts nothing and only runs when {@code BANC_CREATURES} is set, because its numbers
 * depend on the machine. It sets down a herd on a stone floor with one character in the middle — the character
 * is what turns the hunters on — and calls every update subscriber by hand, timing each one. Twice, once empty
 * and once full, so that the difference divided by the head count is the cost of a creature and nothing else.
 * <p>
 * Rendering is out of reach here (the engine is headless), but {@code SkeletonRenderer.update}, the half of the
 * skeleton that runs on the CPU every frame, is a subscriber like the others and is measured with them.
 * <p>
 * {@code BANC_CREATURES=250 ./gradlew :modules:Bestiaire:test --tests '*CoutParCreatureTest' -i}
 */
@Tag("MteTest")
@EnabledIfEnvironmentVariable(named = "BANC_CREATURES", matches = "\\d+")
@IntegrationEnvironment(dependencies = {"Bestiaire"})
public class CoutParCreatureTest {

    private static final String[] ESPECES = {
        "brute", "cerf", "cerfCristal", "champion", "chien", "faisan", "gorgone", "lapin", "lezard", "loup",
        "millepattes", "mouflon", "mouflonTondu", "ours", "rampant", "sanglier", "treant", "vache"
    };

    private static final float DELTA = 1f / 60f;
    private static final int CHAUFFE = 600;
    private static final int MESURE = 1200;
    private static final int COTE = 48;

    @In
    private EntityManager entityManager;
    @In
    private WorldProvider worldProvider;
    @In
    private BlockManager blockManager;
    @In
    private ComponentSystemManager systemes;
    @In
    private EventSystem eventSystem;
    @In
    private MainLoop mainLoop;

    @Test
    public void mesurer() {
        int nombre = Integer.parseInt(System.getenv("BANC_CREATURES"));

        mainLoop.runUntil(mainLoop.makeBlocksRelevant(new BlockRegion(-COTE, -8, -COTE).union(COTE, 8, COTE)));
        Block pierre = blockManager.getBlock("CoreAssets:Stone");
        for (int x = -COTE; x <= COTE; x++) {
            for (int z = -COTE; z <= COTE; z++) {
                worldProvider.setBlock(new Vector3i(x, 0, z), pierre);
            }
        }
        LocationComponent ou = new LocationComponent();
        ou.setWorldPosition(new Vector3f(0, 1.5f, 0));
        entityManager.create(ou, new CharacterComponent(), new AliveCharacterComponent());

        // Sans ecran, le moteur n'enregistre aucun systeme CLIENT : on rend au banc les deux qui tournent a chaque
        // image pour chaque bete, sans quoi il mesurerait un serveur dedie et pas une partie solo.
        systemes.register(new SkeletonRenderer());
        systemes.register(new GaitClientSystem());

        Map<String, Long> vide = tourner();
        int entitesVide = compter();

        int cote = (int) Math.ceil(Math.sqrt(nombre));
        float pas = (2f * COTE - 16f) / Math.max(1, cote);
        List<EntityRef> betes = new ArrayList<>();
        for (int i = 0; i < nombre; i++) {
            float x = -COTE + 8f + (i % cote) * pas;
            float z = -COTE + 8f + (i / cote) * pas;
            betes.add(entityManager.create("Bestiaire:" + ESPECES[i % ESPECES.length], new Vector3f(x, 2f, z)));
        }
        Map<Long, Vector3f> depart = new HashMap<>();
        betes.forEach(b -> depart.put(b.getId(), position(b)));
        Map<String, Long> plein = tourner();
        int entitesPlein = compter();

        // Un banc plus rapide parce que plus rien ne bouge serait un mensonge : on compte ce qui a marche,
        // ce qui chasse, et ce dont les os bougent encore d'une image a l'autre.
        long marcheurs = betes.stream().filter(b -> position(b).distance(depart.get(b.getId())) > 1f).count();
        long chasseurs = betes.stream().filter(b -> b.hasComponent(ChaseComponent.class)).count();
        Map<EntityRef, List<Quaternionf>> poses = new HashMap<>();
        betes.forEach(b -> poses.put(b, pose(b)));
        passe(5, 0);
        long animees = betes.stream().filter(b -> !pose(b).equals(poses.get(b))).count();

        long totalVide = total(vide);
        long totalPlein = total(plein);
        StringBuilder rapport = new StringBuilder();
        rapport.append(String.format("%n== %d créatures, %d entités à vide, %d pleines (%.1f par créature)%n",
                nombre, entitesVide, entitesPlein, (entitesPlein - entitesVide) / (float) Math.max(1, nombre)));
        rapport.append(String.format("%-48s %10s %10s %12s%n", "système", "vide µs", "plein µs", "µs/créature"));
        plein.entrySet().stream()
                .sorted((a, b) -> Long.compare(b.getValue(), a.getValue()))
                .limit(20)
                .forEach(e -> {
                    double v = vide.getOrDefault(e.getKey(), 0L) / 1000.0 / MESURE;
                    double p = e.getValue() / 1000.0 / MESURE;
                    rapport.append(String.format("%-48s %10.1f %10.1f %12.4f%n", e.getKey(), v, p,
                            (p - v) / Math.max(1, nombre)));
                });
        double v = totalVide / 1000.0 / MESURE;
        double p = totalPlein / 1000.0 / MESURE;
        rapport.append(String.format("%-48s %10.1f %10.1f %12.4f%n", "TOTAL", v, p, (p - v) / Math.max(1, nombre)));
        rapport.append(String.format("%d ont marche plus d'un bloc, %d chassent, %d ont des os qui bougent%n",
                marcheurs, chasseurs, animees));
        System.out.println(rapport);
        betes.forEach(EntityRef::destroy);
    }

    /**
     * Warms up, then measures three times and keeps the pass whose total is the median.
     * <p>
     * One pass was not enough on a shared machine: two runs of the same build disagreed by half on the physics
     * line. The median of three throws out a pass that a garbage collection or a neighbour landed in.
     */
    private Map<String, Long> tourner() {
        passe(CHAUFFE, 0);
        List<Map<String, Long>> passes = new ArrayList<>();
        for (int i = 0; i < 3; i++) {
            passes.add(passe(0, MESURE));
        }
        passes.sort((a, b) -> Long.compare(total(a), total(b)));
        return passes.get(1);
    }

    private static long total(Map<String, Long> temps) {
        return temps.values().stream().mapToLong(Long::longValue).sum();
    }

    /** Runs {@code chauffe} frames untimed, then returns the nanoseconds each subscriber spent over {@code mesure}. */
    private Map<String, Long> passe(int chauffe, int mesure) {
        Map<String, Long> temps = new LinkedHashMap<>();
        for (int image = 0; image < chauffe + mesure; image++) {
            eventSystem.process();
            for (UpdateSubscriberSystem systeme : systemes.iterateUpdateSubscribers()) {
                long debut = System.nanoTime();
                systeme.update(DELTA);
                long duree = System.nanoTime() - debut;
                if (image >= chauffe) {
                    temps.merge(systeme.getClass().getSimpleName(), duree, Long::sum);
                }
            }
        }
        return temps;
    }

    private static Vector3f position(EntityRef bete) {
        return bete.getComponent(LocationComponent.class).getWorldPosition(new Vector3f());
    }

    private static List<Quaternionf> pose(EntityRef bete) {
        List<Quaternionf> pose = new ArrayList<>();
        SkeletalMeshComponent squelette = bete.getComponent(SkeletalMeshComponent.class);
        if (squelette != null && squelette.boneEntities != null) {
            squelette.boneEntities.values().forEach(os -> pose.add(
                    new Quaternionf(os.getComponent(LocationComponent.class).getLocalRotation())));
        }
        return pose;
    }

    private int compter() {
        int n = 0;
        for (EntityRef ignored : entityManager.getAllEntities()) {
            n++;
        }
        return n;
    }
}
