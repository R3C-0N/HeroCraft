// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocraft.ui.legende;

import org.joml.Matrix4f;
import org.joml.Quaternionf;
import org.joml.Vector3f;
import org.terasology.engine.entitySystem.entity.EntityBuilder;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.entitySystem.systems.UpdateSubscriberSystem;
import org.terasology.engine.logic.characters.CharacterHeldItemComponent;
import org.terasology.engine.logic.characters.VisualCharacterComponent;
import org.terasology.engine.logic.location.Location;
import org.terasology.engine.logic.location.LocationComponent;
import org.terasology.engine.logic.players.FirstPersonHeldItemTransformComponent;
import org.terasology.engine.logic.players.ItemIsRemotelyHeldComponent;
import org.terasology.engine.logic.players.RemotePersonHeldItemTransformComponent;
import org.terasology.engine.registry.In;
import org.terasology.engine.rendering.logic.LightComponent;
import org.terasology.engine.rendering.logic.MeshComponent;
import org.terasology.engine.rendering.logic.SkeletalMeshComponent;
import org.terasology.engine.rendering.logic.VisualComponent;
import org.terasology.engine.world.block.items.BlockItemComponent;
import org.terasology.gestalt.entitysystem.component.Component;

import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;

/**
 * Puts what a hero holds in the hand of its body, seen from outside: one's own body in third person, and everyone
 * else's. The item rides the right arm's bone, so it swings with the walk, and sits in the fist as {@link Prise}
 * says, as in the creator.
 * <p>
 * The engine shows other players' items at a fixed point beside the body; on a hero that copy is left without its
 * mesh or its light, and this one takes its place.
 * <p>
 * The item's place is set every frame without saving, as the engine does for the bones it rides: nothing listens
 * for it, and the renderer reads it where it is.
 */
@RegisterSystem(RegisterMode.CLIENT)
public class ArmeTenueSystem extends BaseComponentSystem implements UpdateSubscriberSystem {

    private static final String OS = "brasD";
    /** Metres per pattern unit: the hero mesh is drawn 1.80 m tall, 32 units. */
    private static final float UNITE = 1.8f / 32;

    @In
    private EntityManager entityManager;

    private final Map<EntityRef, Port> ports = new HashMap<>();

    @Override
    public void update(float delta) {
        Set<EntityRef> vus = new HashSet<>();
        for (EntityRef personnage : entityManager.getEntitiesWith(CharacterHeldItemComponent.class,
                VisualCharacterComponent.class)) {
            EntityRef corps = personnage.getComponent(VisualCharacterComponent.class).visualCharacter;
            SkeletalMeshComponent squelette = corps.getComponent(SkeletalMeshComponent.class);
            EntityRef os = squelette == null || squelette.boneEntities == null ? null
                    : squelette.boneEntities.get(OS);
            EntityRef objet = personnage.getComponent(CharacterHeldItemComponent.class).selectedItem;
            if (os == null || !os.exists() || !objet.exists() || !objet.hasComponent(MeshComponent.class)) {
                continue;
            }
            vus.add(personnage);
            Port port = ports.get(personnage);
            if (port == null || !port.porte.exists() || !port.objet.equals(objet) || !port.corps.equals(corps)) {
                lacher(port);
                port = prendre(corps, objet);
                ports.put(personnage, port);
            }
            placer(port, squelette, os);
        }
        ports.entrySet().removeIf(entree -> {
            if (vus.contains(entree.getKey())) {
                return false;
            }
            lacher(entree.getValue());
            return true;
        });
        for (EntityRef copie : entityManager.getEntitiesWith(ItemIsRemotelyHeldComponent.class)) {
            if (vus.contains(copie.getComponent(ItemIsRemotelyHeldComponent.class).remotePlayer)) {
                if (copie.hasComponent(MeshComponent.class)) {
                    copie.removeComponent(MeshComponent.class);
                }
                if (copie.hasComponent(LightComponent.class)) {
                    copie.removeComponent(LightComponent.class);
                }
            }
        }
    }

    /** A copy of what the item looks like, as the engine makes for a held item, on the body. */
    private Port prendre(EntityRef corps, EntityRef objet) {
        EntityBuilder builder = entityManager.newBuilder();
        builder.setPersistent(false);
        for (Component component : objet.iterateComponents()) {
            if (component instanceof VisualComponent && !(component instanceof FirstPersonHeldItemTransformComponent)
                    && !(component instanceof RemotePersonHeldItemTransformComponent)) {
                builder.addComponent(component);
            }
        }
        builder.addComponent(new LocationComponent());
        EntityRef porte = builder.build();
        Location.attachChild(corps, porte, new Vector3f(), new Quaternionf());
        return new Port(objet, corps, porte, objet.hasComponent(BlockItemComponent.class));
    }

    private static void lacher(Port port) {
        if (port != null && port.porte.exists()) {
            port.porte.destroy();
        }
    }

    /**
     * The skeleton's own way to the body — its offset and scale, then the bones down to the arm — then from the
     * arm's bone to the pattern, and the grip.
     */
    private static void placer(Port port, SkeletalMeshComponent squelette, EntityRef os) {
        Matrix4f m = new Matrix4f()
                .translation(0, squelette.heightOffset, 0)
                .scale(squelette.scale)
                .translate(squelette.translate)
                .mul(versCorps(os, port.corps))
                .scale(UNITE)
                .translate(-Prise.EPAULE.x, -Prise.EPAULE.y, -Prise.EPAULE.z)
                .mul(Prise.objet(port.bloc, new Matrix4f()));
        LocationComponent lieu = port.porte.getComponent(LocationComponent.class);
        if (lieu == null) {
            return;
        }
        lieu.setLocalPosition(m.getTranslation(new Vector3f()));
        lieu.setLocalRotation(m.getNormalizedRotation(new Quaternionf()));
        // the body's widening stretches across; the item keeps its shape and takes the height's scale
        lieu.setLocalScale((float) Math.cbrt(Math.abs(m.determinant3x3())));
    }

    /** The bone's transform in the body's frame: its own, then each parent's, up to the body. */
    private static Matrix4f versCorps(EntityRef os, EntityRef corps) {
        Matrix4f m = new Matrix4f();
        EntityRef e = os;
        while (e.exists() && !e.equals(corps)) {
            LocationComponent lieu = e.getComponent(LocationComponent.class);
            if (lieu == null) {
                break;
            }
            m = new Matrix4f().translationRotateScale(lieu.position, lieu.rotation, lieu.scale).mul(m);
            e = lieu.getParent();
        }
        return m;
    }

    private static final class Port {
        private final EntityRef objet;
        private final EntityRef corps;
        private final EntityRef porte;
        private final boolean bloc;

        private Port(EntityRef objet, EntityRef corps, EntityRef porte, boolean bloc) {
            this.objet = objet;
            this.corps = corps;
            this.porte = porte;
            this.bloc = bloc;
        }
    }
}
