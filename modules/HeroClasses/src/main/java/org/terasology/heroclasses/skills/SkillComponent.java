// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import com.google.common.collect.Lists;
import org.terasology.gestalt.entitysystem.component.Component;

import java.util.List;

/**
 * A skill, as a prefab. What it does is not here but in the shape components beside it — {@link ArcComponent},
 * {@link LineComponent}, {@link CircleComponent}, {@link DashComponent} — so that a new skill is a new prefab
 * assembled from shapes that already exist, never a new system.
 * <p>
 * Its identifier is the prefab's URN; {@code SkillCatalog} also answers to the prefab's bare name.
 */
public class SkillComponent implements Component<SkillComponent> {

    public String name = "";

    public String description = "";

    /** The prefab of the {@link ClassComponent} this skill belongs to. */
    public String classId = "";

    /** 1 and 2 for the base and derived skills, 3 for fusions, 4 for the ultimate. */
    public int tier = 1;

    /** {@code melee}, {@code ranged} or {@code magic}: the family whose affinity will scale it (D61). */
    public String family = "melee";

    /** Weapon types that wake it; empty means the class's. */
    public List<String> weapons = Lists.newArrayList();

    /** Skills that must be known before this one can be learned, by prefab URN or bare name. */
    public List<String> requires = Lists.newArrayList();

    /** Seconds before it can be cast again. */
    public float cooldown = 5f;

    /** What a hit deals, as a multiple of the held weapon's damage. */
    public float power = 1f;

    @Override
    public void copyFrom(SkillComponent other) {
        this.name = other.name;
        this.description = other.description;
        this.classId = other.classId;
        this.tier = other.tier;
        this.family = other.family;
        this.weapons = Lists.newArrayList(other.weapons);
        this.requires = Lists.newArrayList(other.requires);
        this.cooldown = other.cooldown;
        this.power = other.power;
    }
}
