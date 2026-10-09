// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.cubeworlds;

import org.joml.Vector3i;
import org.joml.Vector3ic;
import org.terasology.engine.integrationenvironment.MainLoop;
import org.terasology.engine.world.WorldProvider;
import org.terasology.engine.world.chunks.Chunks;

import java.nio.charset.StandardCharsets;
import java.util.Locale;

/**
 * One number for what a generator puts in a handful of chunks: every block, by name, in order.
 * <p>
 * It says nothing about whether the world is good, only whether it is the same. That is exactly
 * what a change meant to leave the world alone has to prove — a refactoring of the generator, a
 * fix that should only act at a coarser scale — and a census of volumes cannot: two different
 * worlds can carve the same share of rock.
 */
final class WorldFingerprint {

    private static final long FNV_OFFSET = 0xcbf29ce484222325L;
    private static final long FNV_PRIME = 0x100000001b3L;

    private WorldFingerprint() {
    }

    /** Generates the chunks, then hashes them; prints one line per chunk so a change can be located. */
    static long of(WorldProvider world, MainLoop mainLoop, String label, Vector3ic... chunks) {
        long all = FNV_OFFSET;
        for (Vector3ic chunk : chunks) {
            Vector3i origin = new Vector3i(chunk).mul(Chunks.SIZE_X, Chunks.SIZE_Y, Chunks.SIZE_Z);
            mainLoop.forceAndWaitForGeneration(new Vector3i(origin).add(Chunks.SIZE_X / 2, Chunks.SIZE_Y / 2,
                    Chunks.SIZE_Z / 2));
            long one = FNV_OFFSET;
            for (int y = 0; y < Chunks.SIZE_Y; y++) {
                for (int z = 0; z < Chunks.SIZE_Z; z++) {
                    for (int x = 0; x < Chunks.SIZE_X; x++) {
                        String uri = world.getBlock(origin.x + x, origin.y + y, origin.z + z).getURI().toString();
                        one = mix(one, uri);
                    }
                }
            }
            System.out.printf(Locale.ROOT, "%s chunk %s : %016x%n", label, chunk, one);
            all = (all ^ one) * FNV_PRIME;
        }
        System.out.printf(Locale.ROOT, "%s empreinte : %016x%n", label, all);
        return all;
    }

    private static long mix(long hash, String text) {
        long h = hash;
        for (byte b : text.getBytes(StandardCharsets.UTF_8)) {
            h = (h ^ (b & 0xff)) * FNV_PRIME;
        }
        // A separator, so that "ab" + "c" and "a" + "bc" do not hash alike.
        return (h ^ 0xff) * FNV_PRIME;
    }
}
