// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses;

import org.joml.Vector3f;
import org.junit.jupiter.api.Tag;
import org.junit.jupiter.api.Test;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.integrationenvironment.jupiter.IntegrationEnvironment;
import org.terasology.engine.logic.health.DestroyEvent;
import org.terasology.engine.logic.health.EngineDamageTypes;
import org.terasology.engine.registry.In;
import org.terasology.herobestiary.CadavreComponent;
import org.terasology.herocommon.CreativeModeComponent;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Killing pays the killer what the beast was worth, the peaceful ones and creative mode pay nothing, and the body
 * the bestiary keeps is still kept.
 */
@Tag("MteTest")
@IntegrationEnvironment(dependencies = {"HeroClasses"})
public class ExperienceTest {

    @In
    private EntityManager entityManager;

    @Test
    public void unLoupRapporteSaSanteEtLaisseSonCorps() {
        EntityRef chasseur = chasseur();
        EntityRef loup = tuer("HeroBestiary:loup", chasseur);

        assertEquals(40, experience(chasseur).experience);
        assertTrue(loup.hasComponent(CadavreComponent.class), "the wolf left no body");
    }

    @Test
    public void troisLoupsFontUnNiveauEtUnPoint() {
        EntityRef chasseur = chasseur();
        for (int i = 0; i < 3; i++) {
            tuer("HeroBestiary:loup", chasseur);
        }
        ExperienceComponent experience = experience(chasseur);
        assertEquals(120, experience.experience);
        assertEquals(2, experience.level);
        assertEquals(1, experience.skillPoints);
    }

    @Test
    public void unLapinNeRapporteRien() {
        EntityRef chasseur = chasseur();
        tuer("HeroBestiary:lapin", chasseur);
        assertEquals(0, experience(chasseur).experience);
    }

    @Test
    public void leCreatifNeGagneRien() {
        EntityRef chasseur = chasseur();
        chasseur.addComponent(new CreativeModeComponent());
        tuer("HeroBestiary:loup", chasseur);
        assertEquals(0, experience(chasseur).experience);
    }

    private EntityRef chasseur() {
        EntityRef chasseur = entityManager.create();
        chasseur.addComponent(new ExperienceComponent());
        return chasseur;
    }

    private EntityRef tuer(String espece, EntityRef chasseur) {
        EntityRef bete = entityManager.create(espece, new Vector3f(0, 2, 0));
        bete.send(new DestroyEvent(chasseur, EntityRef.NULL, EngineDamageTypes.PHYSICAL.get()));
        return bete;
    }

    private static ExperienceComponent experience(EntityRef chasseur) {
        return chasseur.getComponent(ExperienceComponent.class);
    }
}
