// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.bestiaire;

import org.joml.Quaternionf;
import org.joml.Vector3f;
import org.terasology.engine.network.Replicate;
import org.terasology.gestalt.entitysystem.component.Component;

/**
 * A creature going backwards, with no say in the matter.
 * <p>
 * Its presence is the whole protocol: while it is there {@link ReculAuthoritySystem} writes the position, and
 * every system that walks a creature stands aside. The rest is the shove itself, fixed at the moment of the
 * blow — where it goes, how far, how high — and the little that moves frame by frame.
 * <p>
 * Only the presence is replicated. The client reads it to stop the legs (see {@link GaitClientSystem}); the
 * position and the lean arrive through the {@code LocationComponent}, which is sent anyway.
 */
public class ReculComponent implements Component<ReculComponent> {

    /** Seconds since the blow. */
    @Replicate
    public float ecoule;

    /** Seconds the backward slide takes, from full speed to a standstill. */
    public float duree;

    /** Blocks travelled by the end of the slide. */
    public float distance;

    /** Blocks already travelled, so that each frame moves by the difference. */
    public float parcouru;

    /** Flat unit vector, away from whoever struck. */
    public Vector3f direction = new Vector3f();

    /** Upward speed, in blocks per second: given by the blow, taken by gravity. */
    public float montee;

    /** Height of the feet, carried separately because the body above them is tilted. */
    public float pieds;

    /** Whether the feet are down. A slide that ends in the air waits for the landing. */
    public boolean pose;

    /** Whether a wall has stopped the slide. The rest of the shove is then spent on the spot. */
    public boolean bute;

    /** Largest lean, in radians, reached halfway through the slide. */
    public float inclinaison;

    /** How the body faced when struck, upright — what the lean is applied to, and what it returns to. */
    public Quaternionf assise = new Quaternionf();

    @Override
    public void copyFrom(ReculComponent other) {
        this.ecoule = other.ecoule;
        this.duree = other.duree;
        this.distance = other.distance;
        this.parcouru = other.parcouru;
        this.direction = new Vector3f(other.direction);
        this.montee = other.montee;
        this.pieds = other.pieds;
        this.pose = other.pose;
        this.bute = other.bute;
        this.inclinaison = other.inclinaison;
        this.assise = new Quaternionf(other.assise);
    }
}
