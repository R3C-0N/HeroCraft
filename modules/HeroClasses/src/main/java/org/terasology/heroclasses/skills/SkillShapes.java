// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.heroclasses.skills;

import org.joml.Vector3f;
import org.joml.Vector3fc;

/**
 * Which points a skill's shape covers, with nothing but vectors, so the geometry is tested on its own.
 * <p>
 * <strong>Every shape is read flat.</strong> The aim is laid on the horizontal before anything is measured: a
 * sweep aimed at the feet or the sky still sweeps the ground in front, and only a band of {@link #HEIGHT}
 * above and below the caster counts. Looking down to swing must not shorten the swing.
 * <p>
 * <strong>Aim assist is a wide reticle, not a lock</strong> (#163): a target within {@link #ASSIST} degrees of
 * the aim and within reach pulls the aim onto itself; nothing outside that cone does, and nothing turns the
 * caster.
 */
public final class SkillShapes {

    /** Blocks above or below the caster's feet a target may stand and still be in the shape. */
    public static final float HEIGHT = 2.5f;

    /** Half-width, in degrees, of the cone in which a target draws the aim. */
    public static final float ASSIST = 12f;

    private SkillShapes() {
    }

    /** The aim laid on the horizontal and normalised; straight up or down falls back on {@code +X}. */
    public static Vector3f flat(Vector3fc aim) {
        Vector3f flat = new Vector3f(aim.x(), 0, aim.z());
        return flat.lengthSquared() < 1e-8f ? new Vector3f(1, 0, 0) : flat.normalize();
    }

    public static boolean inArc(Vector3fc caster, Vector3fc forward, Vector3fc target, float range, float angle) {
        Vector3f offset = offset(caster, target);
        if (offset == null || offset.lengthSquared() > range * range) {
            return false;
        }
        if (offset.lengthSquared() < 1e-6f) {
            return true;
        }
        float cos = offset.normalize().dot(forward);
        return cos >= Math.cos(Math.toRadians(angle / 2));
    }

    public static boolean inLine(Vector3fc caster, Vector3fc forward, Vector3fc target, float length, float width) {
        Vector3f offset = offset(caster, target);
        if (offset == null) {
            return false;
        }
        float along = offset.dot(forward);
        float across = Math.abs(offset.x * forward.z() - offset.z * forward.x());
        return along >= 0 && along <= length && across <= width / 2;
    }

    public static boolean inCircle(Vector3fc caster, Vector3fc target, float radius) {
        Vector3f offset = offset(caster, target);
        return offset != null && offset.lengthSquared() <= radius * radius;
    }

    /**
     * The aim, turned onto the target nearest to it if one stands within {@link #ASSIST} degrees and
     * {@code reach}; otherwise the flat aim unchanged.
     */
    public static Vector3f assist(Vector3fc caster, Vector3fc aim, Iterable<? extends Vector3fc> targets, float reach) {
        Vector3f forward = flat(aim);
        float best = (float) Math.cos(Math.toRadians(ASSIST));
        Vector3f chosen = null;
        for (Vector3fc target : targets) {
            Vector3f offset = offset(caster, target);
            if (offset == null || offset.lengthSquared() > reach * reach || offset.lengthSquared() < 1e-6f) {
                continue;
            }
            float cos = offset.normalize().dot(forward);
            if (cos > best) {
                best = cos;
                chosen = offset;
            }
        }
        return chosen == null ? forward : chosen;
    }

    /** The flat offset from caster to target, or {@code null} when the target is out of the height band. */
    private static Vector3f offset(Vector3fc caster, Vector3fc target) {
        if (Math.abs(target.y() - caster.y()) > HEIGHT) {
            return null;
        }
        return new Vector3f(target.x() - caster.x(), 0, target.z() - caster.z());
    }
}
