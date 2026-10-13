// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import com.google.common.collect.Lists;
import org.terasology.gestalt.entitysystem.component.Component;

import java.util.List;

/**
 * A class, as a prefab: its name, the weapons its skills answer to, and the split it starts a character on.
 * <p>
 * A class is a starting profile, not a cage (D60): nothing here stops a character from learning another class's
 * skills. What it fixes is which weapons wake them — a skill that names none takes its class's.
 */
public class ClassComponent implements Component<ClassComponent> {

    public String name = "";

    /** {@code WeaponComponent.type} values, any of which unlocks the class's skills. */
    public List<String> weapons = Lists.newArrayList();

    /** The starting split, out of a hundred. */
    public int melee;
    public int ranged;
    public int magic;

    @Override
    public void copyFrom(ClassComponent other) {
        this.name = other.name;
        this.weapons = Lists.newArrayList(other.weapons);
        this.melee = other.melee;
        this.ranged = other.ranged;
        this.magic = other.magic;
    }
}
