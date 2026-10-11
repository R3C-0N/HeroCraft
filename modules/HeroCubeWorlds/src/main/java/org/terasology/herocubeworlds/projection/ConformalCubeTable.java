// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds.projection;

import org.terasology.context.annotation.API;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.herocubeworlds.topology.FaceBasis;

/**
 * The conformal projection of a cube face onto the sphere, tabulated.
 * <p>
 * In the stereographic coordinate taken from the centre of a face, the eight corners of the cube
 * are the roots of {@code phi(z) = z^8 + 14 z^4 + 1}. A flat surface offers 270 degrees at a corner
 * where the sphere offers 360, hence a three-quarter power behaviour and the Schwarz-Christoffel
 * equation {@code dz/dw = K phi(z)^(1/4)}, integrated here by Runge-Kutta.
 * <p>
 * Conformal alone is not enough. A pure conformal map shrinks cells to nothing at the corners: the
 * blocks melt into a smooth surface. Equiangular alone turns them into rhombi. So the table blends
 * the two, conformal away from the corners and equiangular inside a disc of radius
 * {@link #BLEND_RADIUS} around each, with a smoothstep in between.
 * <p>
 * What is stored is <em>not</em> {@code z} but the gnomonic pair {@code (a, b)} of the tangent
 * plane, where the edge of a face is exactly {@code a = +-1}. Blending two maps in these
 * coordinates leaves the edge on the edge of the cube; blending in {@code z} would cut the chord
 * and open the seams.
 * <p>
 * The third channel carries the tangential scale factor {@code lambda}. A conformal map does not
 * preserve length, so a radius of {@code R + (y - y0)} would stretch the two tangential directions
 * while leaving the radial one alone, and blocks would stop being cubic. Scaling the height by
 * {@code lambda} turns the deformation into a local similarity: cubes stay cubic, only their size
 * varies.
 * <p>
 * Values are stored as {@code float} on purpose. That is exactly what the shader will read, and
 * keeping doubles here would be a divergence, not a precision.
 */
@API
public final class ConformalCubeTable {

    /** Side of the table. Odd, so that the centre of a face is tabulated rather than interpolated. */
    public static final int RESOLUTION = 513;

    /**
     * The Schwarz-Christoffel constant, {@code integral from 0 to sqrt(2)-1 of
     * dy / (y^8 + 14 y^4 + 1)^(1/4)}.
     * <p>
     * It fixes the radius: a face spans a quarter turn, so {@code R = edge / (4 K)}. Roundness is
     * not a setting.
     */
    public static final double K = 0.4066832501450499;

    /** Radius of the corner blend disc, in units of half a face. */
    public static final double BLEND_RADIUS = 0.25;

    /** Channels per texel: a, b, lambda. */
    public static final int CHANNELS = 3;

    private static final double TARGET_ERROR = 1e-13;
    private static final int NEWTON_STEPS = 12;

    private final float[] data;
    private final int resolution;

    private ConformalCubeTable(float[] data, int resolution) {
        this.data = data;
        this.resolution = resolution;
    }

    /** @return the sphere radius, in blocks, for a face of the given edge in blocks */
    public static double radiusFor(double faceEdgeBlocks) {
        return faceEdgeBlocks / (4.0 * K);
    }

    /** Builds the table at the default resolution. */
    public static ConformalCubeTable build() {
        return build(RESOLUTION, BLEND_RADIUS);
    }

    /**
     * Builds the table.
     * <p>
     * The central column is integrated first, from {@code z = 0} outwards in both directions; then
     * each row is integrated from its point on that column, right then left. Integrating rows from
     * a single corner instead would pile up the error along the way.
     */
    public static ConformalCubeTable build(int resolution, double blendRadius) {
        if (resolution % 2 == 0) {
            throw new IllegalArgumentException("resolution must be odd so the face centre is tabulated");
        }
        int n = resolution;
        double h = 2.0 / (n - 1);
        int mid = (n - 1) / 2;
        float[] data = new float[CHANNELS * n * n];

        double[] columnRe = new double[n];
        double[] columnIm = new double[n];
        double[] z = new double[2];

        columnRe[mid] = 0.0;
        columnIm[mid] = 0.0;
        z[0] = 0.0;
        z[1] = 0.0;
        for (int j = mid + 1; j < n; j++) {
            rk4(z, 0.0, h);
            columnRe[j] = z[0];
            columnIm[j] = z[1];
        }
        z[0] = 0.0;
        z[1] = 0.0;
        for (int j = mid - 1; j >= 0; j--) {
            rk4(z, 0.0, -h);
            columnRe[j] = z[0];
            columnIm[j] = z[1];
        }

        for (int j = 0; j < n; j++) {
            double t = -1.0 + 2.0 * j / (n - 1);

            z[0] = columnRe[j];
            z[1] = columnIm[j];
            deposit(data, n, mid, j, z, blendRadius);
            for (int i = mid + 1; i < n; i++) {
                rk4(z, h, 0.0);
                deposit(data, n, i, j, z, blendRadius);
            }

            z[0] = columnRe[j];
            z[1] = columnIm[j];
            for (int i = mid - 1; i >= 0; i--) {
                rk4(z, -h, 0.0);
                deposit(data, n, i, j, z, blendRadius);
            }
            // t is only read by deposit, through the index; kept for clarity of the sweep
            assert t >= -1.0 && t <= 1.0;
        }

        ConformalCubeTable table = new ConformalCubeTable(data, n);
        table.fillLambda();
        return table;
    }

    private static void deposit(float[] data, int n, int i, int j, double[] z, double blendRadius) {
        double s = -1.0 + 2.0 * i / (n - 1);
        double t = -1.0 + 2.0 * j / (n - 1);
        double[] conformal = gnomonic(z[0], z[1]);
        double equiA = Math.tan(s * Math.PI / 4.0);
        double equiB = Math.tan(t * Math.PI / 4.0);
        double l = blendWeight(s, t, blendRadius);
        int base = CHANNELS * (j * n + i);
        data[base] = (float) (l * conformal[0] + (1.0 - l) * equiA);
        data[base + 1] = (float) (l * conformal[1] + (1.0 - l) * equiB);
    }

    /**
     * Fills the third channel with the tangential scale factor, measured on the table itself rather
     * than derived analytically — inside the blend the map is no longer holomorphic, so there is no
     * formula to trust.
     * <p>
     * The two directions are measured separately and combined by their <em>geometric</em> mean. The
     * radial scale is the only thing a single factor can set, so it can make the deformation an
     * exact local similarity wherever the map is conformal — which is everywhere outside the eight
     * blend discs, where the two directions agree to four decimals. Inside them they differ by up
     * to 2.6, and no radial choice keeps a block cubic; the geometric mean is then the compromise
     * that at least preserves its volume. An arithmetic mean would quietly hide which of the two
     * directions is the one collapsing.
     */
    private void fillLambda() {
        int n = resolution;
        double h = 2.0 / (n - 1);
        for (int j = 0; j < n; j++) {
            double t = -1.0 + 2.0 * j / (n - 1);
            for (int i = 0; i < n; i++) {
                double s = -1.0 + 2.0 * i / (n - 1);
                double alongS = directionalScale(s, t, h, true);
                double alongT = directionalScale(s, t, h, false);
                data[CHANNELS * (j * n + i) + 2] = (float) Math.sqrt(alongS * alongT);
            }
        }
    }

    /**
     * The size of a one-block step, in blocks, along one of the two grid directions.
     * <p>
     * Independent of the face edge: the world step is {@code R |d dir/ds| ds} and the grid step is
     * {@code (E/2) ds}, and {@code R = E / (4 K)}, so the ratio is {@code |d dir/ds| / (2 K)}.
     */
    public double directionalScale(double s, double t, double step, boolean alongS) {
        double s0 = alongS ? Math.max(-1.0, s - step) : s;
        double s1 = alongS ? Math.min(1.0, s + step) : s;
        double t0 = alongS ? t : Math.max(-1.0, t - step);
        double t1 = alongS ? t : Math.min(1.0, t + step);
        double span = alongS ? s1 - s0 : t1 - t0;
        if (span == 0.0) {
            return 0.0;
        }
        double[] d0 = localDirection(s0, t0);
        double[] d1 = localDirection(s1, t1);
        double dx = d1[0] - d0[0];
        double dy = d1[1] - d0[1];
        double dz = d1[2] - d0[2];
        return Math.sqrt(dx * dx + dy * dy + dz * dz) / span / (2.0 * K);
    }

    /** The nominal table step, the right one for {@link #directionalScale}. */
    public double step() {
        return 2.0 / (resolution - 1);
    }

    // -- the differential equation ---------------------------------------------------------------

    private static void derivative(double zr, double zi, double[] out) {
        // z^2
        double z2r = zr * zr - zi * zi;
        double z2i = 2.0 * zr * zi;
        // z^4
        double z4r = z2r * z2r - z2i * z2i;
        double z4i = 2.0 * z2r * z2i;
        // z^8
        double z8r = z4r * z4r - z4i * z4i;
        double z8i = 2.0 * z4r * z4i;
        // phi = z^8 + 14 z^4 + 1
        double pr = z8r + 14.0 * z4r + 1.0;
        double pi = z8i + 14.0 * z4i;
        // K phi^(1/4), principal branch
        double mod = Math.hypot(pr, pi);
        double arg = Math.atan2(pi, pr);
        double root = Math.pow(mod, 0.25);
        out[0] = K * root * Math.cos(arg / 4.0);
        out[1] = K * root * Math.sin(arg / 4.0);
    }

    private static void rk4(double[] z, double dwRe, double dwIm) {
        double[] k = new double[2];
        derivative(z[0], z[1], k);
        double k1r = k[0];
        double k1i = k[1];

        double h1r = 0.5 * (dwRe * k1r - dwIm * k1i);
        double h1i = 0.5 * (dwRe * k1i + dwIm * k1r);
        derivative(z[0] + h1r, z[1] + h1i, k);
        double k2r = k[0];
        double k2i = k[1];

        double h2r = 0.5 * (dwRe * k2r - dwIm * k2i);
        double h2i = 0.5 * (dwRe * k2i + dwIm * k2r);
        derivative(z[0] + h2r, z[1] + h2i, k);
        double k3r = k[0];
        double k3i = k[1];

        double h3r = dwRe * k3r - dwIm * k3i;
        double h3i = dwRe * k3i + dwIm * k3r;
        derivative(z[0] + h3r, z[1] + h3i, k);
        double k4r = k[0];
        double k4i = k[1];

        double sr = (k1r + 2.0 * k2r + 2.0 * k3r + k4r) / 6.0;
        double si = (k1i + 2.0 * k2i + 2.0 * k3i + k4i) / 6.0;
        z[0] += dwRe * sr - dwIm * si;
        z[1] += dwRe * si + dwIm * sr;
    }

    private static double[] gnomonic(double p, double q) {
        double d = 1.0 + p * p + q * q;
        double x = 2.0 * p / d;
        double y = 2.0 * q / d;
        double zz = (1.0 - p * p - q * q) / d;
        return new double[]{x / zz, y / zz};
    }

    /** 1 far from every corner, 0 at a corner, smoothstep in between. */
    private static double blendWeight(double s, double t, double blendRadius) {
        double ds = 1.0 - Math.abs(s);
        double dt = 1.0 - Math.abs(t);
        double d = Math.sqrt(ds * ds + dt * dt);
        double x = Math.max(0.0, Math.min(1.0, d / blendRadius));
        return x * x * (3.0 - 2.0 * x);
    }

    // -- reading ---------------------------------------------------------------------------------

    /** @return the raw texel data, interleaved as (a, b, lambda), for upload to the GPU */
    public float[] data() {
        return data;
    }

    public int resolution() {
        return resolution;
    }

    /**
     * Bilinear read of the table, written out by hand.
     * <p>
     * Deliberately not delegated to hardware filtering: the shader does the very same four fetches
     * and the very same weights, and that identity is the only reason a position computed on the
     * CPU and a vertex placed by the GPU end up in the same spot.
     *
     * @param dest receives (a, b, lambda)
     */
    public void sample(double s, double t, double[] dest) {
        double n = resolution - 1;
        double x = Math.max(0.0, Math.min(n - 1e-9, (s + 1.0) * 0.5 * n));
        double y = Math.max(0.0, Math.min(n - 1e-9, (t + 1.0) * 0.5 * n));
        int i = (int) x;
        int j = (int) y;
        double fx = x - i;
        double fy = y - j;
        for (int c = 0; c < CHANNELS; c++) {
            double a = data[CHANNELS * (j * resolution + i) + c];
            double b = data[CHANNELS * (j * resolution + i + 1) + c];
            double cc = data[CHANNELS * ((j + 1) * resolution + i) + c];
            double d = data[CHANNELS * ((j + 1) * resolution + i + 1) + c];
            double low = a + (b - a) * fx;
            double high = cc + (d - cc) * fx;
            dest[c] = low + (high - low) * fy;
        }
    }

    /** @return the unit direction in the face's own frame, (right, up, normal) */
    public double[] localDirection(double s, double t) {
        double[] ab = new double[CHANNELS];
        sample(s, t, ab);
        double len = Math.sqrt(1.0 + ab[0] * ab[0] + ab[1] * ab[1]);
        return new double[]{ab[0] / len, ab[1] / len, 1.0 / len};
    }

    /** @return the tangential scale factor at a point of a face */
    public double lambda(double s, double t) {
        double[] ab = new double[CHANNELS];
        sample(s, t, ab);
        return ab[2];
    }

    /**
     * @param dest receives the unit direction in planet space
     */
    public void direction(int face, double s, double t, double[] dest) {
        double[] local = localDirection(s, t);
        FaceBasis basis = FaceBasis.BASES[face];
        dest[0] = basis.nx() * local[2] + basis.rx() * local[0] + basis.hx() * local[1];
        dest[1] = basis.ny() * local[2] + basis.ry() * local[0] + basis.hy() * local[1];
        dest[2] = basis.nz() * local[2] + basis.rz() * local[0] + basis.hz() * local[1];
    }

    // -- the way back ----------------------------------------------------------------------------

    /**
     * Newton on the table, with the jacobian taken by finite differences rather than analytically:
     * inside the blend the map is no longer holomorphic, so the analytic jacobian is simply wrong
     * there.
     *
     * @param dest receives (s, t)
     */
    public void fromLocalDirection(double lr, double lu, double ln, double[] dest) {
        double targetA = lr / ln;
        double targetB = lu / ln;
        double s = clampUnit(Math.atan(targetA) / (Math.PI / 4.0));
        double t = clampUnit(Math.atan(targetB) / (Math.PI / 4.0));
        double h = 2.0 / (resolution - 1);
        double[] ab = new double[CHANNELS];
        double[] abS = new double[CHANNELS];
        double[] abT = new double[CHANNELS];

        for (int step = 0; step < NEWTON_STEPS; step++) {
            sample(s, t, ab);
            double e0 = ab[0] - targetA;
            double e1 = ab[1] - targetB;
            if (Math.abs(e0) + Math.abs(e1) < TARGET_ERROR) {
                break;
            }
            double sj = s + h <= 1.0 ? s + h : s - h;
            double tj = t + h <= 1.0 ? t + h : t - h;
            double signS = s + h <= 1.0 ? 1.0 : -1.0;
            double signT = t + h <= 1.0 ? 1.0 : -1.0;
            sample(sj, t, abS);
            sample(s, tj, abT);
            double j00 = signS * (abS[0] - ab[0]) / h;
            double j10 = signS * (abS[1] - ab[1]) / h;
            double j01 = signT * (abT[0] - ab[0]) / h;
            double j11 = signT * (abT[1] - ab[1]) / h;
            double det = j00 * j11 - j01 * j10;
            if (Math.abs(det) < 1e-15) {
                break;
            }
            double ds = -(j11 * e0 - j01 * e1) / det;
            double dt = -(j00 * e1 - j10 * e0) / det;
            double norm = Math.hypot(ds, dt);
            if (norm > 0.25) {
                ds *= 0.25 / norm;
                dt *= 0.25 / norm;
            }
            s = clampUnit(s + ds);
            t = clampUnit(t + dt);
        }
        dest[0] = s;
        dest[1] = t;
    }

    /**
     * @param dest receives (face, s, t); the face is the one whose normal dominates
     */
    public void fromDirection(double x, double y, double z, double[] dest) {
        int best = 0;
        double bestDot = -Double.MAX_VALUE;
        for (int face = 0; face < CubePattern.FACES; face++) {
            FaceBasis b = FaceBasis.BASES[face];
            double dot = x * b.nx() + y * b.ny() + z * b.nz();
            if (dot > bestDot) {
                bestDot = dot;
                best = face;
            }
        }
        FaceBasis b = FaceBasis.BASES[best];
        double lr = x * b.rx() + y * b.ry() + z * b.rz();
        double lu = x * b.hx() + y * b.hy() + z * b.hz();
        double ln = bestDot;
        double[] st = new double[2];
        fromLocalDirection(lr, lu, ln, st);
        dest[0] = best;
        dest[1] = st[0];
        dest[2] = st[1];
    }

    /**
     * Integrates the differential equation straight along the diagonal, to the corner of the face.
     * <p>
     * This is the one check that tests the equation and {@link #K} on their own, with no table and
     * no blend in the way: the answer is known in closed form.
     *
     * @return the stereographic coordinate reached at the corner
     */
    public static double[] integrateCorner(int steps) {
        double[] z = {0.0, 0.0};
        double h = 1.0 / steps;
        for (int i = 0; i < steps; i++) {
            rk4(z, h, h);
        }
        return z;
    }

    /**
     * @return where theory puts the corner: the stereographic image of {@code (1,1,1)/sqrt(3)},
     *         which is {@code 1 / (sqrt(3) + 1)} on both axes
     */
    public static double[] expectedCorner() {
        double v = 1.0 / (Math.sqrt(3.0) + 1.0);
        return new double[]{v, v};
    }

    /** Exposed for the corner check: the gnomonic image of a stereographic coordinate. */
    public static double[] gnomonicOf(double p, double q) {
        return gnomonic(p, q);
    }

    private static double clampUnit(double v) {
        return Math.max(-1.0, Math.min(1.0, v));
    }
}
