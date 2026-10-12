// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herobestiary;

import org.terasology.gestalt.entitysystem.component.Component;

/**
 * How hard a weapon throws what it strikes, next to a bare hand.
 * <p>
 * Read from the item the striker holds when the blow lands. {@code 1} is an ordinary blow, and an item that
 * carries nothing counts as one: a fist, a pickaxe, a block. A war hammer sends a beast flying, a dagger
 * barely moves it — whatever the damage, which is the other axis and stays the other axis.
 * <p>
 * The weapons themselves live in modules that depend on this one, so they say it in their prefabs.
 */
public class ImpactComponent implements Component<ImpactComponent> {

    /** Multiplier on the knockback a blow from this item gives. */
    public float amplification = 1f;

    @Override
    public void copyFrom(ImpactComponent other) {
        this.amplification = other.amplification;
    }
}
