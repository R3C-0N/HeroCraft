// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses;

import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.event.EventPriority;
import org.terasology.engine.entitySystem.event.Priority;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.logic.health.BeforeDestroyEvent;
import org.terasology.gestalt.entitysystem.event.ReceiveEvent;
import org.terasology.herobestiary.CreatureComponent;
import org.terasology.herobestiary.PredatorComponent;
import org.terasology.herocommon.CreativeModeComponent;
import org.terasology.module.health.components.HealthComponent;

/**
 * Experience is earned on the hostile beasts, by whoever struck the killing blow.
 * <p>
 * <strong>Only the hostile ones count</strong> — those that carry a {@link PredatorComponent}. A deer or a
 * rabbit is meat, not a fight, and paying for it would make the meadow the safest place to level.
 * The training dummy has neither, and is worth nothing.
 * <p>
 * <strong>The priority is not a detail.</strong> {@code CadavreAuthoritySystem} consumes this same event to
 * keep the body, and strips its {@link HealthComponent} on the way: heard after it, the creature would have
 * no health left to be worth anything, and a consumed event would not be heard at all.
 * <p>
 * A builder in creative mode earns nothing: a level reached by flying over a horde is not one.
 */
@RegisterSystem(RegisterMode.AUTHORITY)
public class ExperienceAuthoritySystem extends BaseComponentSystem {

    @Priority(EventPriority.PRIORITY_HIGH)
    @ReceiveEvent(components = {CreatureComponent.class, PredatorComponent.class})
    public void onKill(BeforeDestroyEvent event, EntityRef creature, HealthComponent health) {
        EntityRef killer = event.getInstigator();
        if (!killer.exists() || killer.hasComponent(CreativeModeComponent.class)) {
            return;
        }
        gain(killer, Progression.rewardFor(health.maxHealth));
    }

    /**
     * Adds experience and hands out the points of every level crossed.
     *
     * @return whether the character can earn experience at all
     */
    public static boolean gain(EntityRef character, long amount) {
        ExperienceComponent experience = character.getComponent(ExperienceComponent.class);
        if (experience == null) {
            return false;
        }
        if (amount <= 0) {
            return true;
        }
        int before = experience.level;
        experience.experience += amount;
        experience.level = Progression.levelFor(experience.experience);
        experience.skillPoints += (experience.level - before) * Progression.POINTS_PER_LEVEL;
        character.saveComponent(experience);
        if (experience.level > before) {
            character.send(new LevelUpEvent(before, experience.level));
        }
        return true;
    }
}
