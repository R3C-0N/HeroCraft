// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import org.terasology.engine.network.ServerEvent;
import org.terasology.gestalt.entitysystem.event.Event;

/**
 * A client asks to cast a skill. Only the name travels: the server reads the aim from the character's own gaze,
 * the way an attack does, and decides everything else.
 */
@ServerEvent
public class CastSkillRequest implements Event {
    private String skill;

    public CastSkillRequest() {
    }

    public CastSkillRequest(String skill) {
        this.skill = skill;
    }

    public String getSkill() {
        return skill;
    }
}
