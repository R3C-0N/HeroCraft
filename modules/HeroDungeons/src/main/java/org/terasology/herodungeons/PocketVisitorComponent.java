// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.joml.Vector3f;
import org.terasology.gestalt.entitysystem.component.Component;

/**
 * On a character standing in a pocket: where it came from, and where it goes back to.
 * <p>
 * The way back is a copy of the position taken on the way in, never a position worked out again on
 * leaving. The test bench measured the difference (D29): a copy returns to the same block and the
 * same heading to the bit, wherever the ruin stands, seam or corner included.
 */
public class PocketVisitorComponent implements Component<PocketVisitorComponent> {

    /** The pocket's plot. */
    public int plot = -1;

    /** Where the character stood on the planet when it went in. */
    public Vector3f returnPosition = new Vector3f();

    @Override
    public void copyFrom(PocketVisitorComponent other) {
        this.plot = other.plot;
        this.returnPosition = new Vector3f(other.returnPosition);
    }
}
