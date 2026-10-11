// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herobestiary;

import org.joml.Quaternionf;
import org.joml.Vector3f;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.entitySystem.systems.UpdateSubscriberSystem;
import org.terasology.engine.logic.characters.CharacterHeldItemComponent;
import org.terasology.engine.logic.location.LocationComponent;
import org.terasology.engine.registry.In;
import org.terasology.engine.world.WorldProvider;
import org.terasology.gestalt.entitysystem.event.ReceiveEvent;
import org.terasology.module.health.components.HealthComponent;
import org.terasology.module.health.events.OnDamagedEvent;

import java.util.ArrayList;
import java.util.List;

/**
 * A struck beast is thrown backwards, rears, and comes to rest.
 * <p>
 * <strong>The shove is written, not simulated.</strong> It used to be Bullet's: the body went dynamic for a
 * quarter of a second, took an impulse, and was snatched back when the clock ran out — at whatever speed it
 * still had, so the beast slid at a constant pace and stopped dead. Here the slide is a curve: full speed at
 * the blow, falling away to nothing, the distance being {@code 1 - (1 - u)³} of the whole at fraction
 * {@code u} of the time. That is a speed falling as {@code (1 - u)²}, which is what reads as weight. The body
 * stays kinematic throughout, so there is no hand-over, no velocity to cancel and no landing to correct.
 * <p>
 * <strong>The ground is read the way the legs read it.</strong> Each frame's step asks {@link Ground#under}, as
 * {@link Stride} does: a floor more than {@link #MARCHE} above the feet is a wall and ends the slide where it
 * stands, a lower one is simply fallen to. The hop is a real fall, with the gravity of
 * {@link GravityAuthoritySystem}: a beast shoved off a ledge keeps falling until it lands, and only then is it
 * handed back.
 * <p>
 * <strong>The body rears.</strong> It leans back along the shove — its top thrown the way it is going — up to
 * {@link #CABRE} halfway through the slide, and straightens by the end. The skeletons carry one clip each, so
 * this lean is the only flinch there is. It is written into the rotation, which is replicated already.
 * <p>
 * <strong>Every system that walks a creature stands aside while {@link ReculComponent} is on it.</strong> Two
 * owners of one position do not share it, they tear at it — the same reason {@link ChaseComponent} keeps
 * {@link WanderAuthoritySystem} off a hunting wolf.
 * <p>
 * <strong>The killing blow does not throw.</strong> A carcass is made out of the creature itself and has its
 * own descent to run; a beast therefore dies on the spot.
 * <p>
 * <strong>The push is the weapon's amplification divided by the beast's size.</strong> The size is
 * {@link Stride#taille}, a length taken from the volume of the box, so a rabbit flies, a bear shifts and the
 * demon champion barely flinches. The amplification is the {@link ImpactComponent} of the item the striker
 * holds, {@code 1} for a bare hand: a war hammer throws, a dagger nudges. The damage plays no part — it already
 * has its own consequence, and a sharp blade that hits hard is exactly the weapon that should not shove.
 */
@RegisterSystem(RegisterMode.AUTHORITY)
public class ReculAuthoritySystem extends BaseComponentSystem implements UpdateSubscriberSystem {

    /** Seconds from the blow to a standstill. */
    private static final float DUREE = 0.3f;

    /** Blocks a creature of the reference size is thrown by a bare hand. */
    private static final float DISTANCE = 0.75f;

    /** Height of the hop, in blocks. Enough to unstick the feet, not enough to launch. */
    private static final float BOND = 0.18f;

    /** Largest lean backwards, in radians, for a creature of the reference size. */
    private static final float CABRE = (float) Math.toRadians(14);

    /** Neither a mouse nor a mountain: what the size and the weapon together are allowed to do. */
    private static final float MOINDRE = 0.1f;
    private static final float PLUS = 2.5f;

    /** A floor higher than this above the feet is a wall. A beast thrown backwards does not climb. */
    private static final float MARCHE = 0.55f;

    /**
     * Longest a shove may last, in seconds, landing included. A beast thrown into a shaft falls with it and
     * is handed to gravity at this point rather than held for as long as the fall takes.
     */
    private static final float PLAFOND = 1.5f;

    private static final Vector3f HAUT = new Vector3f(0f, 1f, 0f);

    @In
    private EntityManager entityManager;

    @In
    private WorldProvider worldProvider;

    /**
     * Whether something else is holding this body up. Read by everything that would otherwise walk it.
     */
    static boolean enCours(EntityRef creature) {
        return creature.hasComponent(ReculComponent.class);
    }

    @ReceiveEvent
    public void onDamaged(OnDamagedEvent event, EntityRef bete, CreatureComponent creature) {
        if (bete.hasComponent(ScelleComponent.class) || bete.hasComponent(CadavreComponent.class)) {
            return;
        }
        HealthComponent sante = bete.getComponent(HealthComponent.class);
        if (sante == null || sante.currentHealth <= 0) {
            // Le coup qui tue ne projette pas : le cadavre nait dans la destruction meme, et il a sa propre
            // descente a mener. Deux proprietaires pour un corps, c'est un corps qui tressaute.
            return;
        }
        LocationComponent location = bete.getComponent(LocationComponent.class);
        if (location == null) {
            return;
        }

        float force = force(bete, event.getInstigator());
        ReculComponent recul = bete.getComponent(ReculComponent.class);
        boolean nouveau = recul == null;
        // Un second coup en plein vol repart de la ou en est le corps, mais garde l'assise du premier : la
        // rotation courante est deja penchee, et la reprendre pour reference ne la redresserait jamais.
        if (nouveau) {
            recul = new ReculComponent();
            location.getWorldRotation(recul.assise);
            recul.pieds = location.getWorldPosition(new Vector3f()).y - Stride.demiHauteur(bete);
        }
        recul.direction.set(ecarter(location, event.getInstigator()));
        recul.ecoule = 0f;
        recul.duree = DUREE;
        recul.distance = DISTANCE * force;
        recul.parcouru = 0f;
        recul.montee = (float) Math.sqrt(2f * GravityAuthoritySystem.GRAVITE * BOND * Math.min(force, 1.5f));
        recul.pose = false;
        recul.bute = false;
        recul.inclinaison = CABRE * Math.min(force, 1.5f);
        if (nouveau) {
            bete.addComponent(recul);
        } else {
            bete.saveComponent(recul);
        }
    }

    /**
     * One frame of every shove in progress.
     * <p>
     * The list is copied before it is walked: ending a shove removes the component the iteration is built on.
     */
    @Override
    public void update(float delta) {
        List<EntityRef> voltigeurs = new ArrayList<>();
        entityManager.getEntitiesWith(ReculComponent.class, LocationComponent.class).forEach(voltigeurs::add);

        for (EntityRef bete : voltigeurs) {
            ReculComponent recul = bete.getComponent(ReculComponent.class);
            LocationComponent location = bete.getComponent(LocationComponent.class);
            if (recul == null || location == null) {
                continue;
            }
            recul.ecoule += delta;
            float u = Math.min(1f, recul.ecoule / Math.max(1e-3f, recul.duree));
            Vector3f position = location.getWorldPosition(new Vector3f());
            float demiHauteur = Stride.demiHauteur(bete);

            glisser(recul, position, u);
            tomber(recul, position, delta);
            position.y = recul.pieds + demiHauteur;
            location.setWorldPosition(position);

            float penche = recul.inclinaison * (float) Math.sin(Math.PI * u);
            Vector3f axe = new Vector3f(HAUT).cross(recul.direction);
            Quaternionf rotation = new Quaternionf(recul.assise);
            if (penche != 0f && axe.lengthSquared() > 1e-6f) {
                rotation.premul(new Quaternionf().rotationAxis(penche, axe.normalize()));
            }
            location.setWorldRotation(rotation);
            bete.saveComponent(location);

            if ((u >= 1f && recul.pose) || recul.ecoule >= PLAFOND) {
                // Rendue telle quelle : droite, cinematique, les pieds la ou ils sont. Encore en l'air au
                // plafond, c'est la gravite maison qui finit la chute.
                location.setWorldRotation(recul.assise);
                bete.saveComponent(location);
                bete.removeComponent(ReculComponent.class);
            }
        }
    }

    /** The horizontal part: this frame's share of the curve, unless the ground refuses it. */
    private void glisser(ReculComponent recul, Vector3f position, float u) {
        if (recul.bute) {
            return;
        }
        float reste = 1f - u;
        float voulu = recul.distance * (1f - reste * reste * reste);
        float pas = voulu - recul.parcouru;
        if (pas <= 0f) {
            return;
        }
        float x = position.x + recul.direction.x * pas;
        float z = position.z + recul.direction.z * pas;
        float sol = Ground.under(worldProvider, x, recul.pieds, z);
        if (Float.isNaN(sol) || sol - recul.pieds > MARCHE) {
            // Un mur : la bete s'y arrete net, comme on s'arrete contre un mur, et finit son bond sur place.
            recul.bute = true;
            return;
        }
        position.x = x;
        position.z = z;
        recul.parcouru = voulu;
    }

    /** The vertical part: the hop, the fall that follows it, and the floor that ends both. */
    private void tomber(ReculComponent recul, Vector3f position, float delta) {
        float sol = Ground.under(worldProvider, position.x, recul.pieds, position.z);
        if (Float.isNaN(sol)) {
            // Le monde ne repond plus sous elle : on ne la fait pas tomber dans un chunk absent.
            recul.pose = true;
            return;
        }
        recul.montee -= GravityAuthoritySystem.GRAVITE * delta;
        recul.pieds += recul.montee * delta;
        if (recul.pieds <= sol) {
            recul.pieds = sol;
            recul.montee = 0f;
            recul.pose = true;
        } else {
            recul.pose = false;
        }
    }

    /**
     * Away from whoever struck, flat. A blow from straight above has no direction to give, and the beast then
     * goes out the way it is facing — anything rather than a zero vector, which would leave it rearing on the
     * spot for nothing.
     */
    private static Vector3f ecarter(LocationComponent location, EntityRef frappeur) {
        Vector3f fuite = new Vector3f();
        LocationComponent depuis = frappeur.getComponent(LocationComponent.class);
        if (depuis != null) {
            location.getWorldPosition(fuite).sub(depuis.getWorldPosition(new Vector3f()));
        }
        fuite.y = 0f;
        if (fuite.lengthSquared() < 1e-4f) {
            location.getWorldDirection(fuite);
            fuite.y = 0f;
        }
        if (fuite.lengthSquared() < 1e-4f) {
            fuite.set(0f, 0f, 1f);
        }
        return fuite.normalize();
    }

    /** How hard the blow lands on this beast: the weapon's amplification over the beast's size. */
    private static float force(EntityRef bete, EntityRef frappeur) {
        return Math.max(MOINDRE, Math.min(PLUS, amplification(frappeur) / Stride.taille(bete)));
    }

    /**
     * The amplification of what the striker holds, {@code 1} when it holds nothing that says otherwise. Read at
     * the moment the damage lands, which for a melee blow is the moment of the swing: the held item is the
     * weapon that struck.
     */
    static float amplification(EntityRef frappeur) {
        CharacterHeldItemComponent tenu = frappeur.getComponent(CharacterHeldItemComponent.class);
        if (tenu == null) {
            return 1f;
        }
        ImpactComponent impact = tenu.selectedItem.getComponent(ImpactComponent.class);
        return impact == null ? 1f : Math.max(0f, impact.amplification);
    }
}
