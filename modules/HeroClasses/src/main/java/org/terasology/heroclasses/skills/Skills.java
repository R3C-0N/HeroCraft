// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import org.joml.Vector3fc;
import org.terasology.context.annotation.API;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.prefab.Prefab;

/**
 * Learning and casting, for whatever asks on the server's side: the console today, the talent tree and the
 * skill bar's keys tomorrow.
 */
@API
public interface Skills {

    int MAX_RANK = 3;

    /** Outcome of a learn or a cast; the console reads them back to the player. */
    enum Result {
        DONE("fait"),
        UNKNOWN("compétence inconnue"),
        NOT_LEARNED("compétence non apprise"),
        LOCKED("verrouillée : l'arme de sa classe n'est pas en main"),
        COOLING_DOWN("en recharge"),
        NO_POINT("aucun point à dépenser"),
        MISSING_PREREQUISITE("prérequis manquant"),
        MAX_RANK("déjà au rang maximal"),
        NOT_A_HERO("ce personnage n'a pas de compétences");

        private final String message;

        Result(String message) {
            this.message = message;
        }

        public String message() {
            return message;
        }
    }

    /** Spends a point to learn a skill, or to strengthen a known one up to {@link #MAX_RANK}. */
    Result learn(EntityRef character, String id);

    /** Whether the weapon in hand wakes this skill. */
    boolean isAwake(EntityRef character, Prefab skill);

    /** Casts along the character's gaze. */
    Result cast(EntityRef character, String id);

    /** Casts along a given aim. */
    Result cast(EntityRef character, String id, Vector3fc aim);
}
