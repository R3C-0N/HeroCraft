// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herodungeons;

import org.joml.Vector3fc;

/**
 * A window as a walker meets it: a slab one block deep, standing across an axis, with a rectangle cut in
 * it. Whether a step goes through is decided on the segment from where the character was to where it is,
 * cut against the slab, and only then against the rectangle (D29): a test at the point of arrival never
 * fires once a step is longer than the slab is deep, which is any step on a server that ticks slowly.
 */
final class WindowPlane {

    private final int originX;
    private final int originZ;
    private final int axisX;
    private final int axisZ;
    private final int distance;
    private final int halfWidth;
    private final int bottom;
    private final int height;

    /**
     * @param axisX with {@code axisZ}, the unit step along which the window is met
     * @param distance from the origin to the window, along the axis
     * @param bottom the lowest block of the window
     * @param height how many blocks the window rises from {@code bottom}
     */
    WindowPlane(int originX, int originZ, int axisX, int axisZ, int distance, int halfWidth, int bottom, int height) {
        this.originX = originX;
        this.originZ = originZ;
        this.axisX = axisX;
        this.axisZ = axisZ;
        this.distance = distance;
        this.halfWidth = halfWidth;
        this.bottom = bottom;
        this.height = height;
    }

    /** Whether a block position is part of the window. */
    boolean contains(int x, int y, int z) {
        return along(x, z) == distance && Math.abs(across(x, z)) <= halfWidth && y >= bottom && y < bottom + height;
    }

    /**
     * Whether a body moving from {@code from} to {@code to}, positions taken at the middle of the body,
     * passes through the window or stands in it.
     */
    boolean crossedBy(Vector3fc from, Vector3fc to) {
        double a0 = along(from.x(), from.z());
        double a1 = along(to.x(), to.z());
        double near = distance - 0.5;
        double far = distance + 0.5;
        if (Math.max(a0, a1) < near || Math.min(a0, a1) > far) {
            return false;
        }
        double enter = 0;
        double leave = 1;
        if (a1 != a0) {
            double tNear = (near - a0) / (a1 - a0);
            double tFar = (far - a0) / (a1 - a0);
            enter = Math.max(0, Math.min(tNear, tFar));
            leave = Math.min(1, Math.max(tNear, tFar));
        }
        double t = (enter + leave) / 2;
        double x = from.x() + (to.x() - from.x()) * t;
        double y = from.y() + (to.y() - from.y()) * t;
        double z = from.z() + (to.z() - from.z()) * t;
        if (Math.abs(across(x, z)) > halfWidth + 0.5) {
            return false;
        }
        // The body's block, or the feet a block lower.
        long body = Math.round(y);
        return body >= bottom && body - 1 < bottom + height;
    }

    private double along(double x, double z) {
        return (x - originX) * axisX + (z - originZ) * axisZ;
    }

    private double across(double x, double z) {
        return (x - originX) * axisZ + (z - originZ) * axisX;
    }

    private int along(int x, int z) {
        return (x - originX) * axisX + (z - originZ) * axisZ;
    }

    private int across(int x, int z) {
        return (x - originX) * axisZ + (z - originZ) * axisX;
    }
}
