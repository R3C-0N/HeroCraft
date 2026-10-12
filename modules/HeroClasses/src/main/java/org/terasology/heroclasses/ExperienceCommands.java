// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses;

import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.logic.console.commandSystem.annotations.Command;
import org.terasology.engine.logic.console.commandSystem.annotations.CommandParam;
import org.terasology.engine.logic.console.commandSystem.annotations.Sender;
import org.terasology.engine.logic.permission.PermissionManager;
import org.terasology.engine.network.ClientComponent;

/**
 * Granting experience by hand, to try a level without a hunt.
 */
@RegisterSystem
public class ExperienceCommands extends BaseComponentSystem {

    @Command(shortDescription = "Donne de l'expérience",
            helpText = "Sans valeur, donne le niveau, l'expérience et les points. Avec une valeur, l'ajoute.",
            runOnServer = true,
            requiredPermission = PermissionManager.CHEAT_PERMISSION)
    public String xp(@Sender EntityRef client, @CommandParam(value = "valeur", required = false) Integer value) {
        ClientComponent clientComponent = client.getComponent(ClientComponent.class);
        EntityRef character = clientComponent == null ? EntityRef.NULL : clientComponent.character;
        if (!ExperienceAuthoritySystem.gain(character, value == null ? 0 : value)) {
            return "Ce personnage ne gagne pas d'expérience";
        }
        ExperienceComponent experience = character.getComponent(ExperienceComponent.class);
        return String.format("Niveau %d · %d / %d · %d point(s) à dépenser", experience.level,
                experience.experience, Progression.experienceFor(experience.level + 1), experience.skillPoints);
    }
}
