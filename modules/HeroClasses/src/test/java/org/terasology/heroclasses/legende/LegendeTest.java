// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import org.junit.jupiter.api.Tag;
import org.junit.jupiter.api.Test;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.integrationenvironment.jupiter.IntegrationEnvironment;
import org.terasology.engine.registry.In;
import org.terasology.module.inventory.components.InventoryComponent;
import org.terasology.module.inventory.systems.InventoryUtils;

import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The baptism on the server: a valid one seals the hero, gives the way's split and fills the bag; a refused one
 * changes nothing; a sealed hero cannot be baptised twice.
 */
@Tag("MteTest")
@IntegrationEnvironment(dependencies = {"HeroClasses", "HeroEquipment"})
public class LegendeTest {

    @In
    private EntityManager entityManager;

    @Test
    public void leBaptemeScelleEtDonneLaPanoplie() {
        EntityRef heros = heros();
        heros.send(new BaptemeRequest(nain(), "soldat", "HeroEquipment:FlintSabre"));

        LegendeComponent legende = heros.getComponent(LegendeComponent.class);
        assertTrue(legende.scelle);
        assertEquals("Borin", legende.nom);
        assertEquals("nain", legende.espece);
        assertEquals("HeroClasses:soldat", legende.classe);
        assertEquals(90, legende.melee);
        assertEquals(10, legende.ranged);
        List<String> sac = sac(heros);
        assertTrue(sac.contains("HeroEquipment:FlintSabre"), sac.toString());
        assertTrue(sac.contains("HeroEquipment:FlintShield"), sac.toString());
    }

    @Test
    public void uneArmeHorsDeLArsenalEstRefusee() {
        EntityRef heros = heros();
        heros.send(new BaptemeRequest(nain(), "soldat", "HeroEquipment:ShortBow"));
        assertFalse(heros.getComponent(LegendeComponent.class).scelle);
        assertTrue(sac(heros).isEmpty());
    }

    @Test
    public void uneVoieInconnueEstRefusee() {
        EntityRef heros = heros();
        heros.send(new BaptemeRequest(nain(), "astronome", ""));
        assertFalse(heros.getComponent(LegendeComponent.class).scelle);
    }

    @Test
    public void unHerosScelleNeSeRebaptisePas() {
        EntityRef heros = heros();
        heros.send(new BaptemeRequest(nain(), "soldat", "HeroEquipment:FlintSword"));
        Apparence autre = nain();
        autre.nom = "Autre";
        heros.send(new BaptemeRequest(autre, "archer", ""));

        LegendeComponent legende = heros.getComponent(LegendeComponent.class);
        assertEquals("Borin", legende.nom);
        assertEquals("HeroClasses:soldat", legende.classe);
        assertEquals(2, sac(heros).size());
    }

    private EntityRef heros() {
        EntityRef heros = entityManager.create();
        heros.addComponent(new LegendeComponent());
        heros.addComponent(new InventoryComponent(10));
        return heros;
    }

    private static Apparence nain() {
        Apparence a = Atlas.defaut("nain");
        a.nom = "Borin";
        return a;
    }

    private static List<String> sac(EntityRef heros) {
        List<String> sac = new ArrayList<>();
        for (int i = 0; i < InventoryUtils.getSlotCount(heros); i++) {
            EntityRef item = InventoryUtils.getItemAt(heros, i);
            if (item.exists()) {
                sac.add(item.getParentPrefab().getUrn().toString());
            }
        }
        return sac;
    }
}
