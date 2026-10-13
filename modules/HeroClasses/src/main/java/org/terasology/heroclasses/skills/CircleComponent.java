// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import org.terasology.gestalt.entitysystem.component.Component;

/**
 * A ring around the caster: everything within {@code radius}.
 */
public class CircleComponent implements Component<CircleComponent> {

    /** Blocks from the caster. */
    public float radius = 3f;

    @Override
    public void copyFrom(CircleComponent other) {
        this.radius = other.radius;
    }
}
