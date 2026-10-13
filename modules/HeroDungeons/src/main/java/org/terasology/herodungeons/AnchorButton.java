// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.terasology.engine.input.BindButtonEvent;
import org.terasology.engine.input.DefaultBinding;
import org.terasology.engine.input.RegisterBindButton;
import org.terasology.input.InputType;
import org.terasology.input.Keyboard;

/**
 * The hero's innate anchor power (D12): at a ruin's anchor stone it opens the window onto the past, and
 * in the past it tears the hero out of it. The bench's lever key, {@code P}, kept for the game.
 */
@RegisterBindButton(id = "ancre", description = "Pouvoir d'ancre : ouvrir le passé, ou s'en arracher")
@DefaultBinding(type = InputType.KEY, id = Keyboard.KeyId.P)
public class AnchorButton extends BindButtonEvent {
}
