// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herobestiary;

import org.joml.Vector3f;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.logic.characters.CharacterImpulseEvent;
import org.terasology.engine.logic.characters.CharacterMovementComponent;
import org.terasology.engine.logic.location.LocationComponent;
import org.terasology.gestalt.entitysystem.event.ReceiveEvent;
import org.terasology.module.health.events.OnDamagedEvent;

/**
 * A bite pushes the character back, away from whatever bit.
 * <p>
 * The blow had weight on one side only: a beast struck by the player was thrown, and a player bitten by a
 * bear did not move. The engine already knows how to shove a character — {@link CharacterImpulseEvent} adds to
 * the velocity its movement system integrates, ground friction and walls included — so this is only the
 * direction and the size of the push. The character is lifted off its feet a little, or the ground friction of
 * the very next step would eat the push before it showed.
 * <p>
 * <strong>The bigger the beast, the harder the shove,</strong> in proportion to {@link Stride#taille}, the same
 * size that decides how far the beast itself is thrown. A lizard nudges, a bear staggers you, and the demon
 * champion sends you flying. The bite's damage plays no part, as the weapon's damage plays none in
 * {@link ReculAuthoritySystem}: what pushes is the mass behind the teeth.
 * <p>
 * <strong>To know:</strong> this runs on the authority. In single player and on a listen server's own
 * character that is the whole story; a remote client predicts its own movement and will see the push arrive
 * as a correction from the server rather than as a smooth shove.
 */
@RegisterSystem(RegisterMode.AUTHORITY)
public class ReculJoueurAuthoritySystem extends BaseComponentSystem {

    /** Backward speed given by a bite from a beast of the reference size, in blocks per second. */
    private static final float POUSSEE = 5f;

    /** Upward speed, in blocks per second: enough to leave the ground for a quarter of a second. */
    private static final float ENVOL = 3.5f;

    /** What the size is allowed to do: the smallest biter still moves you, the champion does not launch you. */
    private static final float MOINDRE = 0.5f;
    private static final float PLUS = 2.5f;

    @ReceiveEvent(components = {CharacterMovementComponent.class, LocationComponent.class})
    public void onMordu(OnDamagedEvent event, EntityRef personnage) {
        EntityRef bete = event.getInstigator();
        if (!bete.hasComponent(CreatureComponent.class)) {
            return;
        }
        LocationComponent ici = personnage.getComponent(LocationComponent.class);
        LocationComponent depuis = bete.getComponent(LocationComponent.class);
        if (depuis == null) {
            return;
        }
        Vector3f fuite = ici.getWorldPosition(new Vector3f()).sub(depuis.getWorldPosition(new Vector3f()));
        fuite.y = 0f;
        if (fuite.lengthSquared() < 1e-4f) {
            return;
        }
        fuite.normalize();

        float force = Math.max(MOINDRE, Math.min(PLUS, Stride.taille(bete)));
        personnage.send(new CharacterImpulseEvent(
                new Vector3f(fuite.x * POUSSEE * force, ENVOL * Math.min(force, 1.2f), fuite.z * POUSSEE * force)));
    }
}
