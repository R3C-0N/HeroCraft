// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses;

import org.terasology.engine.network.Replicate;
import org.terasology.gestalt.entitysystem.component.Component;

/**
 * Where a character stands on the climb.
 * <p>
 * The level is kept beside the experience although {@link Progression#levelFor} could derive it: a gain is
 * compared against it to know how many levels were crossed, hence how many points to hand out. The points are
 * the unspent ones; the talent tree will spend them.
 */
public class ExperienceComponent implements Component<ExperienceComponent> {

    @Replicate
    public long experience;

    @Replicate
    public int level = 1;

    @Replicate
    public int skillPoints;

    @Override
    public void copyFrom(ExperienceComponent other) {
        this.experience = other.experience;
        this.level = other.level;
        this.skillPoints = other.skillPoints;
    }
}
