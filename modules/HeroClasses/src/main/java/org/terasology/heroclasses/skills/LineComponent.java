// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import org.terasology.gestalt.entitysystem.component.Component;

/**
 * A strip laid out ahead: everything within {@code length} forward and {@code width} across.
 */
public class LineComponent implements Component<LineComponent> {

    /** Blocks ahead of the caster. */
    public float length = 6f;

    /** Full width of the strip, in blocks. */
    public float width = 1.5f;

    @Override
    public void copyFrom(LineComponent other) {
        this.length = other.length;
        this.width = other.width;
    }
}
