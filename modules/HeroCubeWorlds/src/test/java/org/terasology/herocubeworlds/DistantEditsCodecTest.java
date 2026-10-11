// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocubeworlds;

import org.joml.Vector3i;
import org.junit.jupiter.api.Test;
import org.terasology.herocubeworlds.distant.DistantEdits;
import org.terasology.herocubeworlds.distant.DistantEditsComponent;

import java.lang.reflect.Method;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;

/**
 * The captures survive a save: every cell comes back, and the materials are renumbered by name, since
 * palette numbers are only valid for one session. And what a changed chunk weighs in the save.
 */
class DistantEditsCodecTest {

    @Test
    void aSavedCaptureComesBackCellForCell() throws Exception {
        DistantEdits edits = new DistantEdits();
        short[] cells = new short[32 * 64 * 32];
        // A chunk of layers, as generated ground is, with a tower of a third material through it.
        for (int y = 0; y < 64; y++) {
            for (int z = 0; z < 32; z++) {
                for (int x = 0; x < 32; x++) {
                    int material = y < 40 ? 1 : y < 44 ? 2 : 0;
                    if (x < 2 && z < 2 && y < 60) {
                        material = 3;
                    }
                    cells[x + 32 * (z + 32 * y)] = (short) material;
                }
            }
        }
        Method put = DistantEdits.class.getDeclaredMethod("put", long.class, short[].class);
        Method key = DistantEdits.class.getDeclaredMethod("key", int.class, int.class, int.class);
        put.setAccessible(true);
        key.setAccessible(true);
        put.invoke(edits, (long) key.invoke(null, 150, 0, 150), cells);

        List<String> names = List.of("CoreAssets:Stone", "CoreAssets:Dirt", "CoreAssets:Brick");
        DistantEditsComponent component = new DistantEditsComponent();
        edits.saveInto(component, id -> names.get(id));
        String encoded = component.chunks.values().iterator().next();
        System.out.printf("un chunk modifie pese %d octets dans la sauvegarde, contre %d bruts%n",
                encoded.length(), 2 * cells.length);

        DistantEdits back = new DistantEdits();
        // The next session numbers the same blocks differently.
        back.loadFrom(component, name -> 10 + names.indexOf(name));
        Vector3i origin = new Vector3i(150 * 32, 0, 150 * 32);
        for (int y = 0; y < 64; y++) {
            for (int z = 0; z < 32; z++) {
                for (int x = 0; x < 32; x++) {
                    int was = cells[x + 32 * (z + 32 * y)] - 1;
                    int expected = was < 0 ? DistantEdits.AIR : 10 + was;
                    assertEquals(expected, back.cell(origin.x + x, origin.y + y, origin.z + z));
                }
            }
        }
    }
}
