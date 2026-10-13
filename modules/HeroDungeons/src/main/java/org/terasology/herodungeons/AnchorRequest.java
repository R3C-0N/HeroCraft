// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.terasology.engine.network.ServerEvent;
import org.terasology.gestalt.entitysystem.event.Event;

/**
 * A client asks its character to use the anchor power. Nothing travels with it: the server reads where
 * the character stands and decides whether that opens a window or tears it out of the past.
 */
@ServerEvent
public class AnchorRequest implements Event {
}
