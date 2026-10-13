// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.prefab.Prefab;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.logic.console.commandSystem.annotations.Command;
import org.terasology.engine.logic.console.commandSystem.annotations.CommandParam;
import org.terasology.engine.logic.console.commandSystem.annotations.Sender;
import org.terasology.engine.logic.permission.PermissionManager;
import org.terasology.engine.network.ClientComponent;
import org.terasology.engine.registry.In;
import org.terasology.heroclasses.ExperienceComponent;

import java.util.stream.Collectors;

/**
 * Learning and casting from the console, until the talent tree and the skill bar's keys exist.
 */
@RegisterSystem(RegisterMode.AUTHORITY)
public class SkillCommands extends BaseComponentSystem {

    @In
    private SkillCatalog catalog;
    @In
    private Skills skillSystem;

    @Command(shortDescription = "Liste les compétences",
            helpText = "Les compétences connues, leur rang, la barre fixe et ce qui est verrouillé par l'arme en main.",
            runOnServer = true,
            requiredPermission = PermissionManager.NO_PERMISSION)
    public String competences(@Sender EntityRef client) {
        EntityRef character = character(client);
        SkillsComponent skills = character.getComponent(SkillsComponent.class);
        ExperienceComponent experience = character.getComponent(ExperienceComponent.class);
        if (skills == null || experience == null) {
            return Skills.Result.NOT_A_HERO.message();
        }
        String known = catalog.skills().stream()
                .map(prefab -> line(character, skills, prefab))
                .collect(Collectors.joining("\n"));
        return known + "\nPoints à dépenser : " + experience.skillPoints;
    }

    @Command(shortDescription = "Apprend ou renforce une compétence",
            helpText = "Coûte un point. Une compétence déjà connue gagne un rang, jusqu'à trois.",
            runOnServer = true,
            requiredPermission = PermissionManager.NO_PERMISSION)
    public String apprendre(@Sender EntityRef client, @CommandParam("competence") String id) {
        return id + " : " + skillSystem.learn(character(client), id).message();
    }

    @Command(shortDescription = "Lance une compétence vers le réticule",
            runOnServer = true,
            requiredPermission = PermissionManager.NO_PERMISSION)
    public String lancer(@Sender EntityRef client, @CommandParam("competence") String id) {
        return id + " : " + skillSystem.cast(character(client), id).message();
    }

    private String line(EntityRef character, SkillsComponent skills, Prefab prefab) {
        SkillComponent skill = prefab.getComponent(SkillComponent.class);
        String key = prefab.getUrn().toString();
        int rank = skills.ranks.getOrDefault(key, 0);
        String state = rank == 0 ? "non apprise"
                : "rang " + rank + (skillSystem.isAwake(character, prefab) ? "" : ", verrouillée");
        return String.format("%s (%s) — %s", skill.name, prefab.getUrn().getResourceName(), state);
    }

    private static EntityRef character(EntityRef client) {
        ClientComponent component = client.getComponent(ClientComponent.class);
        return component == null ? EntityRef.NULL : component.character;
    }
}
