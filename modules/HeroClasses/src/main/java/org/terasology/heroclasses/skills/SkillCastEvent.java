// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import org.terasology.gestalt.entitysystem.event.Event;

/**
 * Sent to a character once one of its skills has gone off, with how many targets it hit. The effects and sounds
 * that will dress a skill listen here.
 */
public class SkillCastEvent implements Event {
    private String skill;
    private int hits;

    public SkillCastEvent() {
    }

    public SkillCastEvent(String skill, int hits) {
        this.skill = skill;
        this.hits = hits;
    }

    public String getSkill() {
        return skill;
    }

    public int getHits() {
        return hits;
    }
}
