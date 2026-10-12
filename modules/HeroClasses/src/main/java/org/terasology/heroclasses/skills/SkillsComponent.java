// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import com.google.common.collect.Lists;
import com.google.common.collect.Maps;
import org.terasology.engine.network.Replicate;
import org.terasology.gestalt.entitysystem.component.Component;

import java.util.List;
import java.util.Map;

/**
 * What a character knows and has to hand.
 * <p>
 * <strong>The bar is fixed</strong> (D60): {@link #BAR} slots, filled in the order skills are learned, and a
 * skill stays in its slot whether or not the weapon in hand wakes it. Locked is a state of the slot, not an
 * absence from it — a polyvalent character carries locked slots rather than reshuffling mid-fight.
 * <p>
 * The cooldowns are kept as the game time each skill is ready again, so nothing has to tick them down.
 */
public class SkillsComponent implements Component<SkillsComponent> {

    /** A, E, then 1 to 4 (#166); the ultimate gets its own slot with the talent tree. */
    public static final int BAR = 6;

    /** Known skills by prefab URN, and how many times each was strengthened (1 to 3, #172). */
    @Replicate
    public Map<String, Integer> ranks = Maps.newHashMap();

    /** Prefab URNs in slot order; an empty string is an empty slot. */
    @Replicate
    public List<String> bar = Lists.newArrayList();

    /** Game time, in milliseconds, at which each skill can be cast again. */
    @Replicate
    public Map<String, Long> readyAt = Maps.newHashMap();

    @Override
    public void copyFrom(SkillsComponent other) {
        this.ranks = Maps.newHashMap(other.ranks);
        this.bar = Lists.newArrayList(other.bar);
        this.readyAt = Maps.newHashMap(other.readyAt);
    }
}
