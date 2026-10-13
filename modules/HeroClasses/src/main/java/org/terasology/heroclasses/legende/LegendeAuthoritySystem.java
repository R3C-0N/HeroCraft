// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.legende;

import com.google.common.primitives.UnsignedBytes;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.event.EventPriority;
import org.terasology.engine.entitySystem.event.Priority;
import org.terasology.engine.entitySystem.prefab.Prefab;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.logic.characters.events.ScaleToRequest;
import org.terasology.engine.logic.console.commandSystem.annotations.Command;
import org.terasology.engine.logic.console.commandSystem.annotations.Sender;
import org.terasology.engine.logic.inventory.ItemComponent;
import org.terasology.engine.logic.permission.PermissionManager;
import org.terasology.engine.logic.players.event.OnPlayerSpawnedEvent;
import org.terasology.engine.network.ClientComponent;
import org.terasology.engine.registry.In;
import org.terasology.gestalt.entitysystem.event.ReceiveEvent;
import org.terasology.heroclasses.skills.ClassComponent;
import org.terasology.heroclasses.skills.SkillCatalog;
import org.terasology.module.inventory.systems.InventoryManager;

import java.util.Optional;

/**
 * The baptism, decided on the server.
 * <p>
 * <strong>Sealed once, all at once.</strong> The request carries every choice; each is checked ({@link Bapteme},
 * then the class and its arsenal) before anything changes, and a refusal changes nothing. An accepted baptism
 * fixes the appearance and the name, takes the class's split (D60), and hands over the class's flint-tier kit
 * and the one arsenal weapon picked. A second request on a sealed hero is ignored.
 * <p>
 * <strong>The species' height is the character's.</strong> The engine already scales a character — collider,
 * step, reach and eyes — on {@link ScaleToRequest}, and the local player asks for the height in its own
 * settings on every spawn. For a sealed hero that request is overruled here by the hero's height.
 */
@RegisterSystem(RegisterMode.AUTHORITY)
public class LegendeAuthoritySystem extends BaseComponentSystem {

    @In
    private SkillCatalog catalog;
    @In
    private EntityManager entityManager;
    @In
    private InventoryManager inventoryManager;

    @ReceiveEvent
    public void onBapteme(BaptemeRequest request, EntityRef character, LegendeComponent legende) {
        if (legende.scelle) {
            return;
        }
        Apparence apparence = request.apparence();
        Optional<String> refus = Bapteme.verifier(apparence);
        Prefab classe = catalog.characterClass(request.getClasse()).orElse(null);
        if (refus.isEmpty() && classe == null) {
            refus = Optional.of("voie inconnue");
        }
        ClassComponent voie = classe == null ? null : classe.getComponent(ClassComponent.class);
        if (refus.isEmpty() && !voie.arsenal.isEmpty() && !voie.arsenal.contains(request.getArme())) {
            refus = Optional.of("arme hors de l'arsenal de la voie");
        }
        if (refus.isPresent()) {
            character.send(new BaptemeRefuseEvent(refus.get()));
            return;
        }

        legende.prendre(apparence);
        legende.classe = classe.getUrn().toString();
        legende.melee = voie.melee;
        legende.ranged = voie.ranged;
        legende.magic = voie.magic;
        legende.scelle = true;
        character.saveComponent(legende);

        for (ClassComponent.KitItem kit : voie.kit) {
            donner(character, kit.item, kit.count);
        }
        if (!voie.arsenal.isEmpty()) {
            donner(character, request.getArme(), 1);
        }
        character.send(new ScaleToRequest(legende.taille));
    }

    @ReceiveEvent
    public void onSpawn(OnPlayerSpawnedEvent event, EntityRef character, LegendeComponent legende) {
        if (legende.scelle) {
            character.send(new ScaleToRequest(legende.taille));
        }
    }

    /** Whoever asks for a height, a sealed hero keeps the one it was born with. */
    @Priority(EventPriority.PRIORITY_HIGH)
    @ReceiveEvent
    public void onScaleTo(ScaleToRequest request, EntityRef character, LegendeComponent legende) {
        if (legende.scelle && Math.abs(request.getTargetValue() - legende.taille) > 1e-4f) {
            request.consume();
            character.send(new ScaleToRequest(legende.taille));
        }
    }

    @Command(shortDescription = "Montre la légende du héros",
            runOnServer = true,
            requiredPermission = PermissionManager.NO_PERMISSION)
    public String legende(@Sender EntityRef client) {
        LegendeComponent legende = character(client).getComponent(LegendeComponent.class);
        if (legende == null) {
            return "Ce personnage n'a pas de légende.";
        }
        if (!legende.scelle) {
            return "Pas encore baptisé.";
        }
        String voie = catalog.characterClass(legende.classe)
                .map(prefab -> prefab.getComponent(ClassComponent.class).name).orElse(legende.classe);
        return String.format("%s — %s, %s · %.2f m · mêlée %d, distance %d, magie %d", legende.nom,
                Atlas.espece(legende.espece).nom(), voie, legende.taille, legende.melee, legende.ranged,
                legende.magic);
    }

    @Command(shortDescription = "Lève le sceau du héros",
            helpText = "Le créateur se rouvre ; ce que la voie a donné reste dans le sac. Pour les essais.",
            runOnServer = true,
            requiredPermission = PermissionManager.CHEAT_PERMISSION)
    public String recommencer(@Sender EntityRef client) {
        EntityRef character = character(client);
        LegendeComponent legende = character.getComponent(LegendeComponent.class);
        if (legende == null || !legende.scelle) {
            return "Rien à recommencer.";
        }
        legende.scelle = false;
        character.saveComponent(legende);
        return "Le sceau est levé.";
    }

    private static EntityRef character(EntityRef client) {
        ClientComponent component = client.getComponent(ClientComponent.class);
        return component == null ? EntityRef.NULL : component.character;
    }

    private void donner(EntityRef character, String prefab, int count) {
        if (count < 1) {
            return;
        }
        EntityRef item = entityManager.create(prefab);
        if (!item.exists()) {
            return;
        }
        ItemComponent component = item.getComponent(ItemComponent.class);
        if (component != null && count > 1) {
            component.stackCount = UnsignedBytes.saturatedCast(count);
            item.saveComponent(component);
        }
        if (!inventoryManager.giveItem(character, character, item)) {
            item.destroy();
        }
    }
}
