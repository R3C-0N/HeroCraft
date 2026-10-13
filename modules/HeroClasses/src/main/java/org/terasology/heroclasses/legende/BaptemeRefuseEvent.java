// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import org.terasology.engine.network.OwnerEvent;
import org.terasology.gestalt.entitysystem.event.Event;

/**
 * The server's answer when it turns a baptism down, so the creator can say why and stay open. An accepted one
 * needs no answer: the sealed {@link LegendeComponent} replicates and the creator closes on it.
 */
@OwnerEvent
public class BaptemeRefuseEvent implements Event {
    private String raison;

    public BaptemeRefuseEvent() {
    }

    public BaptemeRefuseEvent(String raison) {
        this.raison = raison;
    }

    public String getRaison() {
        return raison;
    }
}
