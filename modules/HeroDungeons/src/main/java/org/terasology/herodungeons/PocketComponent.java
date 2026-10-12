// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.gestalt.entitysystem.component.Component;

/**
 * One pocket of the past, held open on a plot of {@link PocketPlots}.
 * <p>
 * The entity carries a location at the centre of the plot and a relevance region, so the chunks of
 * the pocket stay loaded for as long as it is open, whether anybody stands in it or not: a group
 * that steps out to drop a hammer comes back to the room it left.
 */
public class PocketComponent implements Component<PocketComponent> {

    /** Which plot of {@link PocketPlots} this pocket occupies. */
    public int plot = -1;

    /**
     * Whether the room has been laid down. Until then the chunks are loading, and the character in
     * {@link #waiting} is sent in the moment they are all there.
     */
    public boolean built;

    /** The character who opened the pocket and is waiting to be let in, or NULL once inside. */
    public EntityRef waiting = EntityRef.NULL;

    @Override
    public void copyFrom(PocketComponent other) {
        this.plot = other.plot;
        this.built = other.built;
        this.waiting = other.waiting;
    }
}
