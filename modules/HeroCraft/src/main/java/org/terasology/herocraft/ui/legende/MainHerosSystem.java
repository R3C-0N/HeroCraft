// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocraft.ui.legende;

import org.joml.Matrix4f;
import org.joml.Quaternionf;
import org.joml.Vector3f;
import org.terasology.engine.entitySystem.entity.EntityBuilder;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.entity.lifecycleEvents.OnActivatedComponent;
import org.terasology.engine.entitySystem.entity.lifecycleEvents.OnChangedComponent;
import org.terasology.engine.entitySystem.prefab.Prefab;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.entitySystem.systems.UpdateSubscriberSystem;
import org.terasology.engine.logic.characters.CharacterHeldItemComponent;
import org.terasology.engine.logic.location.Location;
import org.terasology.engine.logic.location.LocationComponent;
import org.terasology.engine.logic.players.FirstPersonHeldItemTransformComponent;
import org.terasology.engine.logic.players.ItemIsHeldComponent;
import org.terasology.engine.logic.players.LocalPlayer;
import org.terasology.engine.registry.In;
import org.terasology.engine.rendering.assets.material.Material;
import org.terasology.engine.rendering.assets.mesh.Mesh;
import org.terasology.engine.rendering.logic.MeshComponent;
import org.terasology.engine.utilities.Assets;
import org.terasology.engine.world.block.items.BlockItemComponent;
import org.terasology.gestalt.entitysystem.event.ReceiveEvent;
import org.terasology.heroclasses.legende.LegendeComponent;
import org.terasology.heroclasses.legende.Peaux;

/**
 * The empty hand in first person wears the hero's own arm: its sheet's right arm and sleeve, on a box cut the same
 * way, in place of the engine's plain one. An unsealed hero keeps the engine's hand.
 * <p>
 * The engine makes that hand lazily and keeps it to itself, so it is recognised by its prefab when it activates.
 * On a reload the hand can come before the local player is known; a half-second look puts that right.
 * <p>
 * Holding something, a baptised hero keeps that arm in view, its fist on the item's handle. The item stays where
 * the engine shows it, drawn to be read from the eye; the arm, a second mesh tipped as the empty hand is, comes to
 * it.
 */
@RegisterSystem(RegisterMode.CLIENT)
public class MainHerosSystem extends BaseComponentSystem implements UpdateSubscriberSystem {

    private static final String MAIN = "engine:hand";
    /**
     * The engine holds its hand end-on, which shows the palm and little else; tipped further, the arm
     * shows its sleeve along the side, as a hero's arm should. Tipped that far it sinks under the bar, so it
     * is lifted to match.
     */
    private static final Vector3f INCLINAISON = new Vector3f(55, 5, 0);
    private static final Vector3f DECALAGE = new Vector3f(-0.5f, -1.1f, 0);
    /** The engine hand's own scale, which the arm keeps. */
    private static final float ECHELLE = 0.8f;

    @In
    private EntityManager entityManager;

    @In
    private LocalPlayer localPlayer;

    private EntityRef main = EntityRef.NULL;
    private Mesh bras;
    /** What the hand wears now: the sealed hero's material, or nothing for the engine's own. */
    private Material porte;
    private float attente;
    /** The engine's copy of the item in hand, once moved into the fist. */
    private EntityRef tenu = EntityRef.NULL;
    /** The arm that holds it. */
    private EntityRef brasTenant = EntityRef.NULL;

    @Override
    public void update(float delta) {
        tenir();
        attente += delta;
        if (attente < 0.5f) {
            return;
        }
        attente = 0;
        LegendeComponent legende = localPlayer.getCharacterEntity().getComponent(LegendeComponent.class);
        Material voulu = legende != null && legende.scelle ? Peaux.materiau(legende.apparence()) : null;
        if (main.exists() && voulu != porte) {
            habiller();
        }
    }

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

    /**
     * Follows the engine's copy of the item in hand: a new one is moved into the fist, and the arm put back beside
     * it; when it goes — another item, third person, empty hands — the arm goes with it.
     */
    private void tenir() {
        LegendeComponent legende = localPlayer.getCharacterEntity().getComponent(LegendeComponent.class);
        CharacterHeldItemComponent tenue = localPlayer.getCharacterEntity().getComponent(CharacterHeldItemComponent.class);
        EntityRef copie = EntityRef.NULL;
        if (legende != null && legende.scelle && tenue != null && tenue.selectedItem.exists()) {
            for (EntityRef candidat : entityManager.getEntitiesWith(ItemIsHeldComponent.class, MeshComponent.class,
                    LocationComponent.class)) {
                copie = candidat;
            }
        }
        if (copie.equals(tenu) && (!copie.exists() || brasTenant.exists())) {
            return;
        }
        if (brasTenant.exists()) {
            brasTenant.destroy();
        }
        brasTenant = EntityRef.NULL;
        tenu = copie;
        EntityRef monture = copie.exists() ? copie.getComponent(LocationComponent.class).getParent() : EntityRef.NULL;
        if (!monture.exists()) {
            return;
        }

        // The item stays where the engine shows it, which is drawn to be read from the eye; the arm comes to it,
        // tipped as the empty hand is, its fist on the item's handle.
        FirstPersonHeldItemTransformComponent place = copie.getComponent(FirstPersonHeldItemTransformComponent.class);
        Vector3f poignee = Prise.poignee(tenue.selectedItem.hasComponent(BlockItemComponent.class));
        if (place != null) {
            new Matrix4f().translationRotateScale(place.translate, rotation(place.rotateDegrees), place.scale)
                    .transformPosition(poignee);
        }
        Quaternionf inclinaison = rotation(INCLINAISON);
        Vector3f poing = new Matrix4f().rotation(inclinaison).scale(ECHELLE)
                .mul(Figure.versBras(new Matrix4f()))
                .transformPosition(new Vector3f(Prise.POING));
        Vector3f epaule = poignee.sub(poing);

        if (bras == null || bras.isDisposed()) {
            bras = Figure.bras();
        }
        MeshComponent mesh = new MeshComponent();
        mesh.mesh = bras;
        mesh.material = Peaux.materiau(legende.apparence());
        EntityBuilder builder = entityManager.newBuilder();
        builder.setPersistent(false);
        builder.addComponent(mesh);
        builder.addComponent(new LocationComponent());
        brasTenant = builder.build();
        Location.attachChild(monture, brasTenant, epaule, inclinaison, ECHELLE);
    }

    private static Quaternionf rotation(Vector3f degres) {
        return new Quaternionf().rotationYXZ((float) Math.toRadians(degres.y), (float) Math.toRadians(degres.x),
                (float) Math.toRadians(degres.z));
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
            porte = mesh.material;
            poser(INCLINAISON, DECALAGE);
        } else {
            mesh.mesh = Assets.getMesh(MAIN).orElse(mesh.mesh);
            mesh.material = Assets.getMaterial(MAIN).orElse(mesh.material);
            porte = null;
            poser(new Vector3f(25, 5, 0), new Vector3f(-0.5f, -1.5f, 0));
        }
        main.saveComponent(mesh);
    }
}
