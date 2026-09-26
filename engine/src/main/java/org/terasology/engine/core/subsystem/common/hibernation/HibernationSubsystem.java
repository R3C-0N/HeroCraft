// Copyright 2021 The Terasology Foundation
// SPDX-License-Identifier: Apache-2.0
package org.terasology.engine.core.subsystem.common.hibernation;

import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.terasology.context.Lifetime;
import org.terasology.engine.context.Context;
import org.terasology.engine.core.modes.GameState;
import org.terasology.engine.core.subsystem.DisplayDevice;
import org.terasology.engine.core.subsystem.EngineSubsystem;
import org.terasology.gestalt.di.ServiceRegistry;

import javax.inject.Inject;


public class HibernationSubsystem implements EngineSubsystem {
    /**
     * Set to "true" and the game never hibernates. Hibernating sleeps 100 ms a frame as soon as the window loses the
     * focus, which is right for a player who has switched away and wrong for a game measured or driven from outside:
     * a click in another window was enough to drop it to nine frames a second in the middle of a measure.
     */
    public static final String DISABLED_PROPERTY = "terasology.hibernation.disabled";

    private static final Logger logger = LoggerFactory.getLogger(HibernationSubsystem.class);
    private HibernationManager hibernationManager;
    private DisplayDevice displayDevice;

    @Inject
    public HibernationSubsystem() {
    }

    @Override
    public String getName() {
        return "Hibernation";
    }

    @Override
    public void preInitialise(ServiceRegistry serviceRegistry) {
        hibernationManager = new HibernationManager();
        serviceRegistry.with(HibernationManager.class).lifetime(Lifetime.Singleton).use(() -> hibernationManager);
    }

    @Override
    public void postInitialise(Context rootContext) {
        displayDevice = rootContext.get(DisplayDevice.class);
        if (Boolean.getBoolean(DISABLED_PROPERTY)) {
            hibernationManager.setHibernationAllowed(false);
        }
    }

    @Override
    public void postUpdate(GameState currentState, float delta) {
        // L'etat de jeu a son mot a dire : `GameState.isHibernationAllowed()`
        // existait deja mais n'etait lu par personne, si bien que les menus
        // tombaient a une dizaine d'images par seconde des que la fenetre
        // perdait le focus — un gel bien reel, et sans rapport avec le jeu.
        boolean allowed = hibernationManager.isHibernationAllowed()
                && (currentState == null || currentState.isHibernationAllowed());

        if (hibernationManager.isHibernating()) {
            if (!allowed || displayDevice.hasFocus()) {
                hibernationManager.setHibernating(false);
            }
        } else {
            if (allowed && !displayDevice.hasFocus()) {
                hibernationManager.setHibernating(true);
            }
        }

        if (hibernationManager.isHibernating()) {
            try {
                Thread.sleep(100);
            } catch (InterruptedException e) {
                logger.warn("Hibernation sleep interrupted", e);
            }
        }
    }
}
