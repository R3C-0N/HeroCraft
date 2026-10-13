// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import org.terasology.context.annotation.API;
import org.terasology.engine.entitySystem.prefab.Prefab;

import java.util.Collection;
import java.util.List;
import java.util.Optional;

/**
 * Every skill and class the game knows, read once from the prefabs. Adding a skill is adding a prefab.
 */
@API
public interface SkillCatalog {

    Collection<Prefab> skills();

    /** By prefab URN ({@code HeroClasses:frappeLourde}) or bare name ({@code frappeLourde}), case aside. */
    Optional<Prefab> skill(String id);

    Optional<Prefab> characterClass(String id);

    /** The weapon types that wake a skill: its own, or its class's when it names none. */
    List<String> weaponsFor(Prefab skill);
}
