// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses;

import org.terasology.gestalt.entitysystem.event.Event;

/**
 * Sent to a character that has just crossed one or more levels, once per gain rather than once per level.
 */
public class LevelUpEvent implements Event {
    private int from;
    private int to;

    public LevelUpEvent() {
    }

    public LevelUpEvent(int from, int to) {
        this.from = from;
        this.to = to;
    }

    public int getFrom() {
        return from;
    }

    public int getTo() {
        return to;
    }
}
