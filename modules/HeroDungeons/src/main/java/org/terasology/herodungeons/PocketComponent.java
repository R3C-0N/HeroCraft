// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.gestalt.entitysystem.component.Component;

import java.util.ArrayList;
import java.util.List;

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

    /** Whether the pocket is the past of a ruin, opened at its anchor stone, rather than the bench of lot 0. */
    public boolean fromRuin;

    /** The ruin: its centre, its floor and the side of its gate, which is all its window needs. */
    public int ruinX;
    public int ruinZ;
    public int floorY;
    public int gateSide;

    /** Game time at which the window closes of itself (D16), in milliseconds; 0 for the bench, which never does. */
    public long closesAt;

    /** Whether the group has been told the window is about to close. */
    public boolean warned;

    /** The characters standing at the anchor stone when the window opened: the only ones who may cross it (D17). */
    public List<EntityRef> members = new ArrayList<>();

    /** The entity at the anchor stone that keeps the ruin loaded while its window is open. */
    public EntityRef keeper = EntityRef.NULL;

    /** Whether the pocket has been emptied and wiped, and only its window is still to be taken down. */
    public boolean closing;

    @Override
    public void copyFrom(PocketComponent other) {
        this.plot = other.plot;
        this.built = other.built;
        this.waiting = other.waiting;
        this.fromRuin = other.fromRuin;
        this.ruinX = other.ruinX;
        this.ruinZ = other.ruinZ;
        this.floorY = other.floorY;
        this.gateSide = other.gateSide;
        this.closesAt = other.closesAt;
        this.warned = other.warned;
        this.members = new ArrayList<>(other.members);
        this.keeper = other.keeper;
        this.closing = other.closing;
    }
}
