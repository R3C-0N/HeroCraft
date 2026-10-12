// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import org.joml.Vector3f;
import org.joml.Vector3fc;
import org.terasology.engine.core.Time;
import org.terasology.engine.entitySystem.entity.EntityManager;
import org.terasology.engine.entitySystem.entity.EntityRef;
import org.terasology.engine.entitySystem.prefab.Prefab;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterMode;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.logic.characters.CharacterHeldItemComponent;
import org.terasology.engine.logic.characters.CharacterImpulseEvent;
import org.terasology.engine.logic.characters.GazeAuthoritySystem;
import org.terasology.engine.logic.health.EngineDamageTypes;
import org.terasology.engine.logic.inventory.ItemComponent;
import org.terasology.engine.logic.location.LocationComponent;
import org.terasology.engine.logic.players.PlayerCharacterComponent;
import org.terasology.engine.registry.In;
import org.terasology.engine.registry.Share;
import org.terasology.gestalt.entitysystem.event.ReceiveEvent;
import org.terasology.heroclasses.ExperienceComponent;
import org.terasology.heroequipment.CharacterStats;
import org.terasology.heroequipment.WeaponComponent;
import org.terasology.heroequipment.Wear;
import org.terasology.module.health.components.HealthComponent;
import org.terasology.module.health.events.DoDamageEvent;

import java.util.ArrayList;
import java.util.List;

/**
 * Learning a skill, and casting it.
 * <p>
 * <strong>Casting is the server's.</strong> The client names a skill and nothing else; the aim is read here from
 * the character's gaze, as an attack's is. A cast is refused, in this order, when the skill is unknown, not
 * learned, not woken by the weapon in hand (D60), or still cooling down. A refused cast costs nothing.
 * <p>
 * <strong>A hit is the held weapon's damage times the skill's power,</strong> sent as an ordinary
 * {@link DoDamageEvent} with the caster as instigator — so armour, damage numbers, the bestiary's knockback and
 * the experience of D60 all hear a skill exactly as they hear a swing. The affinity of D61 will scale it here.
 * <p>
 * <strong>No friendly fire.</strong> Another player is never a target: the game is played among friends (D22),
 * and a sweep through a group should not open a quarrel.
 * <p>
 * Learning costs one point and asks for the skill's prerequisites; learning a known skill again strengthens it,
 * up to {@link #MAX_RANK}. The tiers of #405, the fusions of #407 and the single ultimate of #408 come with the
 * talent tree.
 */
@RegisterSystem(RegisterMode.AUTHORITY)
@Share(Skills.class)
public class SkillAuthoritySystem extends BaseComponentSystem implements Skills {

    /** Horizontal speed a dash gives per block of {@link DashComponent#distance}, and the hop that unsticks it. */
    private static final float DASH_SPEED = 2.5f;
    private static final float DASH_LIFT = 2f;

    @In
    private SkillCatalog catalog;
    @In
    private EntityManager entityManager;
    @In
    private Time time;

    @ReceiveEvent
    public void onCastRequest(CastSkillRequest request, EntityRef character, SkillsComponent skills) {
        cast(character, request.getSkill());
    }

    @Override
    public Result learn(EntityRef character, String id) {
        SkillsComponent skills = character.getComponent(SkillsComponent.class);
        ExperienceComponent experience = character.getComponent(ExperienceComponent.class);
        if (skills == null || experience == null) {
            return Result.NOT_A_HERO;
        }
        Prefab prefab = catalog.skill(id).orElse(null);
        if (prefab == null) {
            return Result.UNKNOWN;
        }
        String key = prefab.getUrn().toString();
        int rank = skills.ranks.getOrDefault(key, 0);
        if (rank >= MAX_RANK) {
            return Result.MAX_RANK;
        }
        if (experience.skillPoints <= 0) {
            return Result.NO_POINT;
        }
        if (rank == 0) {
            for (String required : prefab.getComponent(SkillComponent.class).requires) {
                String requiredKey = catalog.skill(required).map(p -> p.getUrn().toString()).orElse(required);
                if (!skills.ranks.containsKey(requiredKey)) {
                    return Result.MISSING_PREREQUISITE;
                }
            }
            place(skills, key);
        }
        skills.ranks.put(key, rank + 1);
        experience.skillPoints--;
        character.saveComponent(experience);
        character.saveComponent(skills);
        return Result.DONE;
    }

    @Override
    public boolean isAwake(EntityRef character, Prefab skill) {
        WeaponComponent weapon = heldWeapon(character).getComponent(WeaponComponent.class);
        return weapon != null && catalog.weaponsFor(skill).contains(weapon.type);
    }

    @Override
    public Result cast(EntityRef character, String id) {
        EntityRef gaze = GazeAuthoritySystem.getGazeEntityForCharacter(character);
        LocationComponent location = gaze.getComponent(LocationComponent.class);
        Vector3f aim = location == null ? new Vector3f(1, 0, 0) : location.getWorldDirection(new Vector3f());
        return cast(character, id, aim);
    }

    @Override
    public Result cast(EntityRef character, String id, Vector3fc aim) {
        SkillsComponent skills = character.getComponent(SkillsComponent.class);
        LocationComponent location = character.getComponent(LocationComponent.class);
        if (skills == null || location == null) {
            return Result.NOT_A_HERO;
        }
        Prefab prefab = catalog.skill(id).orElse(null);
        if (prefab == null) {
            return Result.UNKNOWN;
        }
        String key = prefab.getUrn().toString();
        if (!skills.ranks.containsKey(key)) {
            return Result.NOT_LEARNED;
        }
        if (!isAwake(character, prefab)) {
            return Result.LOCKED;
        }
        long now = time.getGameTimeInMs();
        if (skills.readyAt.getOrDefault(key, 0L) > now) {
            return Result.COOLING_DOWN;
        }
        SkillComponent skill = prefab.getComponent(SkillComponent.class);
        skills.readyAt.put(key, now + (long) (skill.cooldown * 1000));
        character.saveComponent(skills);

        Vector3f caster = location.getWorldPosition(new Vector3f());
        List<EntityRef> candidates = candidates(character);
        Vector3f forward = SkillShapes.assist(caster, aim, positions(candidates), reach(prefab));
        int damage = Math.max(1, Math.round(weaponDamage(character) * skill.power));

        int hits = 0;
        for (EntityRef target : candidates) {
            Vector3f at = target.getComponent(LocationComponent.class).getWorldPosition(new Vector3f());
            if (covers(prefab, caster, forward, at)) {
                target.send(new DoDamageEvent(damage, EngineDamageTypes.PHYSICAL.get(), character,
                        heldWeapon(character)));
                hits++;
            }
        }
        DashComponent dash = prefab.getComponent(DashComponent.class);
        if (dash != null) {
            character.send(new CharacterImpulseEvent(new Vector3f(forward).mul(dash.distance * DASH_SPEED)
                    .add(0, DASH_LIFT, 0)));
        }
        character.send(new SkillCastEvent(key, hits));
        return Result.DONE;
    }

    /** Puts a newly learned skill in the first free slot of the fixed bar, if one is left. */
    private static void place(SkillsComponent skills, String key) {
        while (skills.bar.size() < SkillsComponent.BAR) {
            skills.bar.add("");
        }
        int free = skills.bar.indexOf("");
        if (free >= 0) {
            skills.bar.set(free, key);
        }
    }

    /** Whether any shape the skill carries covers this point. */
    private static boolean covers(Prefab skill, Vector3fc caster, Vector3fc forward, Vector3fc at) {
        ArcComponent arc = skill.getComponent(ArcComponent.class);
        if (arc != null && SkillShapes.inArc(caster, forward, at, arc.range, arc.angle)) {
            return true;
        }
        LineComponent line = skill.getComponent(LineComponent.class);
        if (line != null && SkillShapes.inLine(caster, forward, at, line.length, line.width)) {
            return true;
        }
        CircleComponent circle = skill.getComponent(CircleComponent.class);
        return circle != null && SkillShapes.inCircle(caster, at, circle.radius);
    }

    /** How far the aim assist may look: the longest of the skill's shapes. */
    private static float reach(Prefab skill) {
        float reach = 0;
        ArcComponent arc = skill.getComponent(ArcComponent.class);
        if (arc != null) {
            reach = Math.max(reach, arc.range);
        }
        LineComponent line = skill.getComponent(LineComponent.class);
        if (line != null) {
            reach = Math.max(reach, line.length);
        }
        return reach;
    }

    /** Everything alive that could be hit: not the caster, not another player. */
    private List<EntityRef> candidates(EntityRef caster) {
        List<EntityRef> found = new ArrayList<>();
        for (EntityRef entity : entityManager.getEntitiesWith(HealthComponent.class, LocationComponent.class)) {
            if (entity.equals(caster) || entity.hasComponent(PlayerCharacterComponent.class)) {
                continue;
            }
            HealthComponent health = entity.getComponent(HealthComponent.class);
            if (health.currentHealth > 0) {
                found.add(entity);
            }
        }
        return found;
    }

    private static List<Vector3f> positions(List<EntityRef> entities) {
        List<Vector3f> positions = new ArrayList<>(entities.size());
        for (EntityRef entity : entities) {
            positions.add(entity.getComponent(LocationComponent.class).getWorldPosition(new Vector3f()));
        }
        return positions;
    }

    /** The item in hand, or nothing when there is none or it is broken. */
    static EntityRef heldWeapon(EntityRef character) {
        CharacterHeldItemComponent held = character.getComponent(CharacterHeldItemComponent.class);
        if (held == null || !held.selectedItem.exists() || Wear.isBroken(held.selectedItem)) {
            return EntityRef.NULL;
        }
        return held.selectedItem;
    }

    private static int weaponDamage(EntityRef character) {
        ItemComponent item = heldWeapon(character).getComponent(ItemComponent.class);
        return item == null ? CharacterStats.UNARMED_DAMAGE : item.baseDamage;
    }
}
