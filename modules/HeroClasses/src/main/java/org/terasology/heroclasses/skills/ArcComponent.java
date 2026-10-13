// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import org.terasology.gestalt.entitysystem.component.Component;

/**
 * A blow that sweeps the front: everything within {@code range} and {@code angle} degrees of the aim.
 */
public class ArcComponent implements Component<ArcComponent> {

    /** Blocks from the caster. */
    public float range = 2.5f;

    /** Full width of the sweep, in degrees. */
    public float angle = 100f;

    @Override
    public void copyFrom(ArcComponent other) {
        this.range = other.range;
        this.angle = other.angle;
    }
}
