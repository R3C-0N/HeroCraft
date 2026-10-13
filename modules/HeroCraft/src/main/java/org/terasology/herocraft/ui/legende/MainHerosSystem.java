// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocraft.ui.legende;

import org.joml.Vector3f;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.entity.lifecycleEvents.OnActivatedComponent;
import org.terasology.engine.entitySystem.entity.lifecycleEvents.OnChangedComponent;
import org.terasology.engine.entitySystem.prefab.Prefab;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.logic.players.FirstPersonHeldItemTransformComponent;
import org.terasology.engine.logic.players.LocalPlayer;
import org.terasology.engine.registry.In;
import org.terasology.engine.rendering.assets.mesh.Mesh;
import org.terasology.engine.rendering.logic.MeshComponent;
import org.terasology.engine.utilities.Assets;
import org.terasology.gestalt.entitysystem.event.ReceiveEvent;
import org.terasology.heroclasses.legende.LegendeComponent;
import org.terasology.heroclasses.legende.Peaux;

/**
 * The empty hand in first person wears the hero's own arm: its sheet's right arm and sleeve, on a box cut the same
 * way, in place of the engine's plain one. An unsealed hero keeps the engine's hand.
 * <p>
 * The engine makes that hand lazily and keeps it to itself, so it is recognised by its prefab when it activates.
 */
@RegisterSystem(RegisterMode.CLIENT)
public class MainHerosSystem extends BaseComponentSystem {

    private static final String MAIN = "engine:hand";
    /**
     * The engine holds its hand end-on, which shows the palm and little else; tipped further and drawn in, the arm
     * shows its sleeve along the side, as a hero's arm should.
     */
    private static final Vector3f INCLINAISON = new Vector3f(55, 5, 0);
    private static final Vector3f DECALAGE = new Vector3f(-0.5f, -1.5f, 0);

    @In
    private LocalPlayer localPlayer;

    private EntityRef main = EntityRef.NULL;
    private Mesh bras;

    @ReceiveEvent(components = FirstPersonHeldItemTransformComponent.class)
    public void onMain(OnActivatedComponent event, EntityRef entity, MeshComponent mesh) {
        Prefab prefab = entity.getParentPrefab();
        if (prefab != null && MAIN.equals(prefab.getUrn().toString())) {
            main = entity;
            habiller();
        }
    }

    @ReceiveEvent
    public void onLegende(OnChangedComponent event, EntityRef character, LegendeComponent legende) {
        if (character.equals(localPlayer.getCharacterEntity())) {
            habiller();
        }
    }

    @ReceiveEvent
    public void onLegendeActive(OnActivatedComponent event, EntityRef character, LegendeComponent legende) {
        if (character.equals(localPlayer.getCharacterEntity())) {
            habiller();
        }
    }

    private void poser(Vector3f rotation, Vector3f translation) {
        FirstPersonHeldItemTransformComponent pose = main.getComponent(FirstPersonHeldItemTransformComponent.class);
        if (pose != null) {
            pose.rotateDegrees = new Vector3f(rotation);
            pose.translate = new Vector3f(translation);
            main.saveComponent(pose);
        }
    }

    private void habiller() {
        MeshComponent mesh = main.getComponent(MeshComponent.class);
        if (mesh == null) {
            return;
        }
        LegendeComponent legende = localPlayer.getCharacterEntity().getComponent(LegendeComponent.class);
        if (legende != null && legende.scelle) {
            if (bras == null || bras.isDisposed()) {
                bras = Figure.bras();
            }
            mesh.mesh = bras;
            mesh.material = Peaux.materiau(legende.apparence());
            poser(INCLINAISON, DECALAGE);
        } else {
            mesh.mesh = Assets.getMesh(MAIN).orElse(mesh.mesh);
            mesh.material = Assets.getMaterial(MAIN).orElse(mesh.material);
            poser(new Vector3f(25, 5, 0), new Vector3f(-0.5f, -1.5f, 0));
        }
        main.saveComponent(mesh);
    }
}
