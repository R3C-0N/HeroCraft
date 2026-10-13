// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocraft.ui.legende;

import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.entitySystem.systems.UpdateSubscriberSystem;
import org.terasology.engine.logic.console.commandSystem.annotations.Command;
import org.terasology.engine.logic.permission.PermissionManager;
import org.terasology.engine.logic.players.LocalPlayer;
import org.terasology.engine.registry.In;
import org.terasology.engine.rendering.nui.NUIManager;
import org.terasology.gestalt.entitysystem.event.ReceiveEvent;
import org.terasology.heroclasses.legende.BaptemeRefuseEvent;
import org.terasology.heroclasses.legende.LegendeComponent;
import org.terasology.nui.UIScreenLayer;

/**
 * Opens the creator on a hero not yet baptised, once per arrival: a player who closes it plays on unsealed until
 * the next arrival, or until {@code createur} brings it back. It also carries the server's answer to the screen.
 * <p>
 * It watches rather than listens: the local character is only known to be the local player's some time after it
 * activates, and a half-second look is cheaper than chasing the order of those events.
 */
@RegisterSystem(RegisterMode.CLIENT)
public class LegendeClientSystem extends BaseComponentSystem implements UpdateSubscriberSystem {

    static final String ECRAN = "HeroCraft:legendeScreen";
    private static final float PERIODE = 0.5f;

    @In
    private LocalPlayer localPlayer;
    @In
    private NUIManager nuiManager;

    private float attente;
    private long suivi;
    private boolean ouvert;
    private boolean annonce;

    @Override
    public void update(float delta) {
        attente += delta;
        if (attente < PERIODE) {
            return;
        }
        attente = 0;
        EntityRef character = localPlayer.getCharacterEntity();
        LegendeComponent legende = character.getComponent(LegendeComponent.class);
        if (legende == null) {
            return;
        }
        if (character.getId() != suivi) {
            suivi = character.getId();
            ouvert = false;
        }
        if (legende.scelle) {
            // a later unsealing, by command, opens the creator again
            ouvert = false;
            if (!annonce && ecran() != null) {
                ecran().scellee();
                annonce = true;
            }
        } else if (!ouvert) {
            nuiManager.pushScreen(ECRAN, LegendeScreen.class);
            ouvert = true;
            annonce = false;
        }
    }

    @ReceiveEvent
    public void onRefus(BaptemeRefuseEvent event, EntityRef character) {
        LegendeScreen ecran = ecran();
        if (ecran != null) {
            ecran.refuser(event.getRaison());
        }
    }

    @Command(shortDescription = "Rouvre le créateur de légende",
            helpText = "Tant que le héros n'est pas baptisé.",
            requiredPermission = PermissionManager.NO_PERMISSION)
    public String createur() {
        LegendeComponent legende = localPlayer.getCharacterEntity().getComponent(LegendeComponent.class);
        if (legende == null) {
            return "Ce personnage n'a pas de légende.";
        }
        if (legende.scelle) {
            return "La légende est scellée.";
        }
        nuiManager.pushScreen(ECRAN, LegendeScreen.class);
        return "";
    }

    private LegendeScreen ecran() {
        if (!nuiManager.isOpen(ECRAN)) {
            return null;
        }
        UIScreenLayer ecran = nuiManager.getScreen(ECRAN);
        return ecran instanceof LegendeScreen ? (LegendeScreen) ecran : null;
    }
}
