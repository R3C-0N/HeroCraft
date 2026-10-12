// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import org.terasology.engine.entitySystem.prefab.Prefab;
import org.terasology.engine.entitySystem.prefab.PrefabManager;
import org.terasology.engine.entitySystem.systems.BaseComponentSystem;
import org.terasology.engine.entitySystem.systems.RegisterSystem;
import org.terasology.engine.registry.In;
import org.terasology.engine.registry.Share;

import java.util.Collection;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Optional;
import java.util.TreeMap;

/**
 * The catalog, built at start-up the way the bestiary is: prefabs do not appear while the game runs.
 */
@RegisterSystem
@Share(SkillCatalog.class)
public class SkillCatalogSystem extends BaseComponentSystem implements SkillCatalog {

    @In
    private PrefabManager prefabManager;

    private final Map<String, Prefab> skills = new TreeMap<>();
    private final Map<String, Prefab> classes = new TreeMap<>();

    @Override
    public void initialise() {
        for (Prefab prefab : prefabManager.listPrefabs(SkillComponent.class)) {
            index(skills, prefab);
        }
        for (Prefab prefab : prefabManager.listPrefabs(ClassComponent.class)) {
            index(classes, prefab);
        }
    }

    @Override
    public Collection<Prefab> skills() {
        return List.copyOf(new LinkedHashSet<>(skills.values()));
    }

    @Override
    public Optional<Prefab> skill(String id) {
        return lookup(skills, id);
    }

    @Override
    public Optional<Prefab> characterClass(String id) {
        return lookup(classes, id);
    }

    @Override
    public List<String> weaponsFor(Prefab skill) {
        SkillComponent component = skill.getComponent(SkillComponent.class);
        if (component == null) {
            return List.of();
        }
        if (!component.weapons.isEmpty()) {
            return List.copyOf(component.weapons);
        }
        return characterClass(component.classId)
                .map(prefab -> List.copyOf(prefab.getComponent(ClassComponent.class).weapons))
                .orElse(List.of());
    }

    /** Each prefab answers to its full URN and to its bare name. */
    private static void index(Map<String, Prefab> into, Prefab prefab) {
        into.put(key(prefab.getUrn().toString()), prefab);
        into.putIfAbsent(key(prefab.getUrn().getResourceName().toString()), prefab);
    }

    private static Optional<Prefab> lookup(Map<String, Prefab> from, String id) {
        return id == null ? Optional.empty() : Optional.ofNullable(from.get(key(id)));
    }

    private static String key(String id) {
        return id.toLowerCase(Locale.ROOT);
    }
}
