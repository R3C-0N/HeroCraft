// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import org.joml.Vector3f;
import org.terasology.engine.entitySystem.entity.EntityBuilder;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.entity.lifecycleEvents.OnAddedComponent;
import org.terasology.engine.entitySystem.entity.lifecycleEvents.OnChangedComponent;
import org.terasology.engine.entitySystem.event.EventPriority;
import org.terasology.engine.entitySystem.event.Priority;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.logic.characters.CharacterComponent;
import org.terasology.engine.logic.characters.VisualCharacterComponent;
import org.terasology.engine.logic.characters.events.CreateVisualCharacterEvent;
import org.terasology.engine.logic.nameTags.NameTagComponent;
import org.terasology.engine.rendering.logic.SkeletalMeshComponent;
import org.terasology.gestalt.entitysystem.event.ReceiveEvent;

/**
 * Puts a baptised hero's body on screen, client side: its painted sheet on the hero mesh, its height and build on
 * the mesh's scale, and its name over its head.
 * <p>
 * The collider and the eyes follow the height through the engine's own {@code ScaleToRequest}, sent by the
 * authority; here only the picture is scaled, and by the same factor, so the feet stay on the ground. An unsealed
 * character keeps the engine's default body.
 */
@RegisterSystem(RegisterMode.CLIENT)
public class HerosVisuelSystem extends BaseComponentSystem {

    /** Height of {@code engine:heros}, feet to crown, in metres: the mesh is drawn for a 1.80 m hero. */
    static final float HAUTEUR_MODELE = 1.8f;

    @ReceiveEvent
    public void onCreerVisuel(CreateVisualCharacterEvent event, EntityRef character, LegendeComponent legende) {
        if (!legende.scelle) {
            return;
        }
        EntityBuilder builder = event.getVisualCharacterBuilder();
        builder.addPrefab("engine:defaultVisualCharacter");
        SkeletalMeshComponent mesh = builder.getComponent(SkeletalMeshComponent.class);
        if (mesh == null) {
            return;
        }
        habiller(mesh, legende);
        builder.saveComponent(mesh);
        event.consume();
    }

    @ReceiveEvent
    public void onLegendeChangee(OnChangedComponent event, EntityRef character, LegendeComponent legende,
                                 VisualCharacterComponent visuel) {
        if (!legende.scelle) {
            return;
        }
        EntityRef corps = visuel.visualCharacter;
        SkeletalMeshComponent mesh = corps.getComponent(SkeletalMeshComponent.class);
        if (mesh != null) {
            habiller(mesh, legende);
            corps.saveComponent(mesh);
        }
        NameTagComponent etiquette = character.getComponent(NameTagComponent.class);
        if (etiquette != null) {
            etiqueter(etiquette, legende, character);
            character.saveComponent(etiquette);
        }
    }

    /** The engine names other players after their account; a hero goes by the name it was baptised with. */
    @Priority(EventPriority.PRIORITY_TRIVIAL)
    @ReceiveEvent
    public void onEtiquette(OnAddedComponent event, EntityRef character, NameTagComponent etiquette,
                            LegendeComponent legende) {
        if (!legende.scelle) {
            return;
        }
        etiqueter(etiquette, legende, character);
        character.saveComponent(etiquette);
    }

    private static void habiller(SkeletalMeshComponent mesh, LegendeComponent legende) {
        mesh.material = Peaux.materiau(legende.apparence());
        mesh.scale = echelle(legende.taille, legende.corpulence);
    }

    private static void etiqueter(NameTagComponent etiquette, LegendeComponent legende, EntityRef character) {
        etiquette.text = legende.nom;
        CharacterComponent personnage = character.getComponent(CharacterComponent.class);
        if (personnage != null) {
            etiquette.yOffset = personnage.nameTagOffset * legende.taille / HAUTEUR_MODELE;
        }
    }

    /**
     * Uniform in height, so a gnome is a small man rather than a squashed one; the build only widens, by eight
     * percent either way, which is as far as one mesh stretches before the arms leave the shoulders.
     */
    static Vector3f echelle(float taille, float corpulence) {
        float s = taille / HAUTEUR_MODELE;
        float largeur = s * (0.92f + 0.16f * corpulence);
        return new Vector3f(largeur, s, largeur);
    }
}
