// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herobestiary;

import org.joml.Vector3f;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.entitySystem.systems.UpdateSubscriberSystem;
import org.terasology.engine.logic.location.LocationComponent;
import org.terasology.engine.registry.In;
import org.terasology.engine.rendering.logic.SkeletalMeshComponent;

import java.util.HashMap;
import java.util.Map;

/**
 * The legs swing at the speed the animal is actually going.
 * <p>
 * The model carries one animation — the maquette's own idle, which for a quadruped already <em>is</em> a
 * stride — so the gait is a rate, not a second clip. At rest the rate falls to zero and the legs stop; running
 * winds them up.
 * <p>
 * This is a client system and it has to be: {@code SkeletalMeshComponent} is a {@code VisualComponent} and
 * {@code animationRate} is not replicated, so setting it on the server would change nothing anyone can see.
 * Nothing is sent for it either — the speed is <em>measured</em>, from positions the client already receives.
 * The measurement is smoothed because those positions arrive at the network's cadence and not at the frame's:
 * read raw, the rate would flicker with every packet.
 */
@RegisterSystem(RegisterMode.CLIENT)
public class GaitClientSystem extends BaseComponentSystem implements UpdateSubscriberSystem {

    /** How fast the smoothed speed catches up, per second. */
    private static final float SUIVI = 6f;

    /** Below this fraction of the walking pace, the animal is standing still. */
    private static final float SEUIL = 0.1f;

    /** The legs never swing more than this many times their nominal rate. */
    private static final float RATIO_MAX = 3.5f;

    @In
    private EntityManager entityManager;

    private final Map<EntityRef, Allure> allures = new HashMap<>();

    /** Where an animal was last frame, and how fast it has been going. One entry, so one lookup a frame. */
    private static final class Allure {
        private final Vector3f derniere = new Vector3f();
        private float vitesse;
    }

    @Override
    public void update(float delta) {
        allures.keySet().removeIf(entity -> !entity.exists());
        if (delta <= 0f) {
            return;
        }

        for (EntityRef creature : entityManager.getEntitiesWith(WanderComponent.class,
                SkeletalMeshComponent.class, LocationComponent.class)) {
            LocationComponent location = creature.getComponent(LocationComponent.class);
            SkeletalMeshComponent maillage = creature.getComponent(SkeletalMeshComponent.class);
            WanderComponent wander = creature.getComponent(WanderComponent.class);
            if (location == null || maillage == null || wander == null) {
                continue;
            }

            Vector3f position = location.getWorldPosition(new Vector3f());
            Allure allure = allures.get(creature);
            if (allure == null) {
                allure = new Allure();
                allure.derniere.set(position);
                allures.put(creature, allure);
                continue;
            }

            if (creature.hasComponent(ReculComponent.class)) {
                // Projetee, elle ne marche pas : la vitesse mesuree est celle du coup, et la traduire en
                // foulees la faisait courir a reculons, pattes a plein regime. Les jambes se figent, et
                // l'allure repart de zero a la reprise plutot que du pic du recul.
                allure.derniere.set(position);
                allure.vitesse = 0f;
                maillage.animationRate = 0f;
                continue;
            }

            float dx = position.x - allure.derniere.x;
            float dz = position.z - allure.derniere.z;
            allure.derniere.set(position);
            float brute = (float) Math.sqrt(dx * dx + dz * dz) / delta;
            allure.vitesse += (brute - allure.vitesse) * Math.min(1f, delta * SUIVI);
            float lissee = allure.vitesse;

            float nominal = wander.speed <= 0f ? 1f : wander.speed;
            float ratio = lissee / nominal;
            // Le composant n'est pas replique : on l'ecrit sans `saveComponent`, qui ne servirait qu'a
            // salir l'entite pour un champ que personne n'envoie.
            maillage.animationRate = ratio < SEUIL ? 0f : Math.min(RATIO_MAX, ratio);
        }
    }
}
