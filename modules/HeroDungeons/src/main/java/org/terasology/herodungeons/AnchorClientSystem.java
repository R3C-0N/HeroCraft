// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.event.ReceiveEvent;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.logic.characters.CharacterComponent;

/** Turns the anchor key into a request to the server, which alone knows the ruins and the pockets. */
@RegisterSystem(RegisterMode.CLIENT)
public class AnchorClientSystem extends BaseComponentSystem {

    @ReceiveEvent(components = CharacterComponent.class)
    public void onAnchorButton(AnchorButton event, EntityRef character) {
        if (event.isDown()) {
            character.send(new AnchorRequest());
            event.consume();
        }
    }
}
