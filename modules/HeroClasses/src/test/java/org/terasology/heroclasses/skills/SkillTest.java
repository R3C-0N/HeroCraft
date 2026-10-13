// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import org.joml.Vector3f;
import org.junit.jupiter.api.Tag;
import org.junit.jupiter.api.Test;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.integrationenvironment.jupiter.IntegrationEnvironment;
import org.terasology.engine.logic.characters.CharacterHeldItemComponent;
import org.terasology.engine.logic.location.LocationComponent;
import org.terasology.engine.registry.In;
import org.terasology.heroclasses.ExperienceComponent;
import org.terasology.module.health.components.HealthComponent;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The Soldat's skills in a world: learning spends a point and fills the bar, the class weapon wakes them, a cast
 * hurts what its shape covers and nothing else, and a cooldown refuses the second cast.
 */
@Tag("MteTest")
@IntegrationEnvironment(dependencies = {"HeroClasses"})
public class SkillTest {

    private static final Vector3f EAST = new Vector3f(1, 0, 0);

    @In
    private EntityManager entityManager;
    @In
    private Skills skills;
    @In
    private SkillCatalog catalog;

    @Test
    public void leCatalogueConnaitLeSoldat() {
        assertTrue(catalog.characterClass("soldat").isPresent());
        assertEquals(4, catalog.skills().size());
        assertTrue(catalog.weaponsFor(catalog.skill("frappeLourde").get()).contains("oneHandedSword"));
    }

    @Test
    public void apprendreCouteUnPointEtRemplitLaBarre() {
        EntityRef soldat = soldat(1);
        assertEquals(Skills.Result.DONE, skills.learn(soldat, "frappeLourde"));
        assertEquals(Skills.Result.NO_POINT, skills.learn(soldat, "briseGarde"));

        SkillsComponent known = soldat.getComponent(SkillsComponent.class);
        assertEquals(1, known.ranks.get("HeroClasses:frappeLourde"));
        assertEquals("HeroClasses:frappeLourde", known.bar.get(0));
        assertEquals(0, soldat.getComponent(ExperienceComponent.class).skillPoints);
    }

    @Test
    public void unRangDePlusJusquATrois() {
        EntityRef soldat = soldat(5);
        for (int i = 0; i < 3; i++) {
            assertEquals(Skills.Result.DONE, skills.learn(soldat, "frappeLourde"));
        }
        assertEquals(Skills.Result.MAX_RANK, skills.learn(soldat, "frappeLourde"));
        assertEquals(1, soldat.getComponent(SkillsComponent.class).bar.stream()
                .filter("HeroClasses:frappeLourde"::equals).count());
    }

    @Test
    public void sansSonArmeLaCompetenceEstVerrouillee() {
        EntityRef soldat = soldat(1);
        assertEquals(Skills.Result.NOT_LEARNED, skills.cast(soldat, "frappeLourde", EAST));
        skills.learn(soldat, "frappeLourde");
        assertEquals(Skills.Result.LOCKED, skills.cast(soldat, "frappeLourde", EAST));
    }

    @Test
    public void laFrappeLourdeBlesseCeQuiEstDevantPasDerriere() {
        EntityRef soldat = arme(soldat(1));
        skills.learn(soldat, "frappeLourde");
        EntityRef devant = entityManager.create("HeroBestiary:loup", new Vector3f(2, 0, 0));
        EntityRef derriere = entityManager.create("HeroBestiary:loup", new Vector3f(-2, 0, 0));

        assertEquals(Skills.Result.DONE, skills.cast(soldat, "frappeLourde", EAST));
        assertTrue(sante(devant) < 40, "the wolf in front was not hit");
        assertEquals(40, sante(derriere), "the wolf behind was hit");
    }

    @Test
    public void laRechargeRefuseLeSecondLancer() {
        EntityRef soldat = arme(soldat(1));
        skills.learn(soldat, "frappeLourde");
        assertEquals(Skills.Result.DONE, skills.cast(soldat, "frappeLourde", EAST));
        assertEquals(Skills.Result.COOLING_DOWN, skills.cast(soldat, "frappeLourde", EAST));
    }

    private EntityRef soldat(int points) {
        EntityRef soldat = entityManager.create();
        ExperienceComponent experience = new ExperienceComponent();
        experience.skillPoints = points;
        soldat.addComponent(experience);
        soldat.addComponent(new SkillsComponent());
        soldat.addComponent(new LocationComponent(new Vector3f()));
        return soldat;
    }

    private EntityRef arme(EntityRef soldat) {
        CharacterHeldItemComponent held = new CharacterHeldItemComponent();
        held.selectedItem = entityManager.create("HeroEquipment:WoodenSword");
        soldat.addComponent(held);
        return soldat;
    }

    private static int sante(EntityRef bete) {
        return bete.getComponent(HealthComponent.class).currentHealth;
    }
}
