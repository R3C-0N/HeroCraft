// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import org.terasology.gestalt.entitysystem.component.Component;

/**
 * The caster is thrown forward along the aim. Paired with a {@link LineComponent} of the same length, it hits what it runs through.
 */
public class DashComponent implements Component<DashComponent> {

    /** Roughly how far the throw carries on flat ground, in blocks. */
    public float distance = 6f;

    @Override
    public void copyFrom(DashComponent other) {
        this.distance = other.distance;
    }
}
