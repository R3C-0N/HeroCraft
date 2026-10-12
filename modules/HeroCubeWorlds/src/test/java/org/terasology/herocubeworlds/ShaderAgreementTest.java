// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.junit.jupiter.api.Test;
import org.terasology.herocubeworlds.projection.ConformalCubeTable;
import org.terasology.herocubeworlds.rendering.CubeSphereProjection;
import org.terasology.herocubeworlds.topology.CubePattern;
import org.terasology.herocubeworlds.topology.FaceBasis;

import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Whether the shader and the CPU agree.
 * <p>
 * Two things can drift apart here, and only one of them is about arithmetic. The six face frames and
 * the layout of the net are transcribed by hand into GLSL, so the test reads them back <em>out of
 * the shader source</em> and compares them to the Java constants: a sign flipped there would put a
 * whole face somewhere else, and nothing else in the build would notice. Then the bilinear read is
 * re-implemented in single precision, the way the shader does it, and compared against the double
 * precision path — because a position computed on the CPU for culling and a vertex placed by the GPU
 * have to land in the same spot.
 */
class ShaderAgreementTest {

    private static final String SHADER = "org/terasology/engine/include/globalFunctionsVertIncl.glsl";

    private static String source() throws IOException {
        try (InputStream in = ShaderAgreementTest.class.getClassLoader().getResourceAsStream(SHADER)) {
            assertNotNull(in, "the engine's vertex include is not on the classpath: " + SHADER);
            return new String(in.readAllBytes(), StandardCharsets.UTF_8);
        }
    }

    /** Pulls the vec3 literals out of a {@code const vec3 NAME[6] = vec3[6](...)} declaration. */
    private static List<double[]> vectors(String source, String name) {
        int start = source.indexOf("const vec3 " + name + "[SPHERE_FACES]");
        assertTrue(start >= 0, "no declaration of " + name + " in the shader");
        int end = source.indexOf(";", start);
        String body = source.substring(start, end);
        Matcher m = Pattern.compile("vec3\\(\\s*(-?[0-9.]+)\\s*,\\s*(-?[0-9.]+)\\s*,\\s*(-?[0-9.]+)\\s*\\)")
                .matcher(body);
        List<double[]> found = new ArrayList<>();
        while (m.find()) {
            found.add(new double[]{
                    Double.parseDouble(m.group(1)),
                    Double.parseDouble(m.group(2)),
                    Double.parseDouble(m.group(3))});
        }
        return found;
    }

    @Test
    void theShaderCarriesTheSameFacesAsTheCpu() throws IOException {
        String source = source();
        List<double[]> right = vectors(source, "SPHERE_RIGHT");
        List<double[]> up = vectors(source, "SPHERE_UP");
        List<double[]> normal = vectors(source, "SPHERE_NORMAL");

        assertEquals(CubePattern.FACES, right.size(), "SPHERE_RIGHT has the wrong number of faces");
        assertEquals(CubePattern.FACES, up.size(), "SPHERE_UP has the wrong number of faces");
        assertEquals(CubePattern.FACES, normal.size(), "SPHERE_NORMAL has the wrong number of faces");

        for (int face = 0; face < CubePattern.FACES; face++) {
            FaceBasis basis = FaceBasis.BASES[face];
            assertVector(basis.rx(), basis.ry(), basis.rz(), right.get(face), "right of face " + face);
            assertVector(basis.hx(), basis.hy(), basis.hz(), up.get(face), "up of face " + face);
            assertVector(basis.nx(), basis.ny(), basis.nz(), normal.get(face), "normal of face " + face);
        }

        // The layout of the net, transcribed as vec2 slots.
        int start = source.indexOf("const vec2 SPHERE_PATTERN[SPHERE_FACES]");
        assertTrue(start >= 0, "no declaration of SPHERE_PATTERN in the shader");
        Matcher m = Pattern.compile("vec2\\(\\s*(-?[0-9.]+)\\s*,\\s*(-?[0-9.]+)\\s*\\)")
                .matcher(source.substring(start, source.indexOf(";", start)));
        int face = 0;
        while (m.find()) {
            assertEquals(CubePattern.PATTERN[face][0], (int) Double.parseDouble(m.group(1)),
                    "column of face " + face);
            assertEquals(CubePattern.PATTERN[face][1], (int) Double.parseDouble(m.group(2)),
                    "row of face " + face);
            face++;
        }
        assertEquals(CubePattern.FACES, face, "SPHERE_PATTERN has the wrong number of faces");
    }

    private static void assertVector(int x, int y, int z, double[] actual, String what) {
        assertEquals(x, (int) actual[0], what + ", x");
        assertEquals(y, (int) actual[1], what + ", y");
        assertEquals(z, (int) actual[2], what + ", z");
    }

    /**
     * The shader's read of the table, transcribed in single precision, against the double precision
     * one. Same four texels, same weights, same clamp.
     */
    /**
     * The pockets of the past sit on dead slots, and both sides send what must not be seen to the same
     * place. A CPU that culled on one exile and a shader that drew at another would decide visibility
     * on evidence from somewhere the vertex is not.
     */
    @Test
    void theShaderExilesWhereTheCpuDoes() throws IOException {
        Matcher m = Pattern.compile("const float SPHERE_EXILE\\s*=\\s*([0-9.eE+-]+)\\s*;").matcher(source());
        assertTrue(m.find(), "no SPHERE_EXILE in the shader");
        assertEquals(CubeSphereProjection.EXILE, Float.parseFloat(m.group(1)));
    }

    @Test
    void theShaderReadsTheSameTable() {
        ConformalCubeTable table = ConformalCubeTable.build();
        float[] data = table.data();
        int size = table.resolution();
        double worst = 0.0;
        double[] cpu = new double[ConformalCubeTable.CHANNELS];

        for (int i = 0; i <= 200; i++) {
            for (int j = 0; j <= 200; j++) {
                // deliberately off the grid nodes, where interpolation actually does something
                double s = -1.0 + 2.0 * (i + 0.37) / 200.0;
                double t = -1.0 + 2.0 * (j + 0.61) / 200.0;
                if (s > 1.0 || t > 1.0) {
                    continue;
                }
                table.sample(s, t, cpu);
                float[] gpu = shaderSample(data, size, (float) s, (float) t);
                for (int c = 0; c < ConformalCubeTable.CHANNELS; c++) {
                    worst = Math.max(worst, Math.abs(cpu[c] - gpu[c]));
                }
            }
        }
        System.out.printf("the shader's read differs from the CPU's by at most %.2e%n", worst);
        assertTrue(worst < 1e-6, "the two reads have drifted apart: " + worst);
    }

    /** A line by line transcription of sphereTableAt from the shader. */
    private static float[] shaderSample(float[] data, int size, float s, float t) {
        float n = size - 1;
        float x = clamp((s + 1.0f) * 0.5f * n, 0.0f, n - 0.0001f);
        float y = clamp((t + 1.0f) * 0.5f * n, 0.0f, n - 0.0001f);
        int ix = (int) x;
        int iy = (int) y;
        float fx = x - ix;
        float fy = y - iy;
        float[] out = new float[ConformalCubeTable.CHANNELS];
        for (int c = 0; c < ConformalCubeTable.CHANNELS; c++) {
            float a = data[ConformalCubeTable.CHANNELS * (iy * size + ix) + c];
            float b = data[ConformalCubeTable.CHANNELS * (iy * size + ix + 1) + c];
            float cc = data[ConformalCubeTable.CHANNELS * ((iy + 1) * size + ix) + c];
            float d = data[ConformalCubeTable.CHANNELS * ((iy + 1) * size + ix + 1) + c];
            out[c] = mix(mix(a, b, fx), mix(cc, d, fx), fy);
        }
        return out;
    }

    private static float mix(float a, float b, float f) {
        return a + (b - a) * f;
    }

    private static float clamp(float v, float lo, float hi) {
        return Math.max(lo, Math.min(hi, v));
    }
}
