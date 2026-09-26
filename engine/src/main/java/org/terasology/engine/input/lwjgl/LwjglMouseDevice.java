// Copyright 2021 The Terasology Foundation
// SPDX-License-Identifier: Apache-2.0

package org.terasology.engine.input.lwjgl;

import com.google.common.collect.Lists;
import gnu.trove.set.TIntSet;
import gnu.trove.set.hash.TIntHashSet;
import org.joml.Vector2d;
import org.joml.Vector2i;
import org.lwjgl.BufferUtils;
import org.lwjgl.glfw.GLFW;
import org.terasology.engine.config.RenderingConfig;
import org.terasology.input.ButtonState;
import org.terasology.input.InputType;
import org.terasology.input.MouseInput;
import org.terasology.input.device.MouseAction;
import org.terasology.input.device.MouseDevice;

import java.beans.PropertyChangeEvent;
import java.beans.PropertyChangeListener;
import java.nio.DoubleBuffer;
import java.util.Queue;

/**
 * Lwjgl 3's (GLFW) mouse device representation.
 * Handles mouse input via GLFW's callbacks.
 * Handles mouse state.
 */
public class LwjglMouseDevice implements MouseDevice, PropertyChangeListener {
    /**
     * Set to "true" and the cursor is never captured, nor followed by the camera: the pointer stays free to leave the
     * window, and the view turns only when told to. For a game driven from outside while somebody works at the same
     * desk. Clicks and the wheel still reach the game.
     */
    public static final String FREE_MOUSE_PROPERTY = "terasology.mouse.free";

    private final boolean freeMouse = Boolean.getBoolean(FREE_MOUSE_PROPERTY);
    private RenderingConfig renderingConfig;
    private float uiScale;
    private boolean mouseGrabbed;
    private Queue<MouseAction> queue = Lists.newLinkedList();

    private TIntSet buttonStates = new TIntHashSet();

    private double xpos;
    private double ypos;

    private double xposDelta;
    private double yposDelta;

    public LwjglMouseDevice(RenderingConfig renderingConfig) {
        this.renderingConfig = renderingConfig;
        this.uiScale = renderingConfig.getUiScale() / 100f;
        renderingConfig.subscribe(RenderingConfig.UI_SCALE, this);
    }

    public void registerToLwjglWindow(long window) {
        GLFW.glfwSetMouseButtonCallback(window, this::mouseButtonCallback);
        GLFW.glfwSetScrollCallback(window, this::scrollCallback);
        GLFW.glfwSetWindowFocusCallback(window, this::focusCallback);
    }

    @Override
    public void update() {
        long window = GLFW.glfwGetCurrentContext();
        DoubleBuffer mouseX = BufferUtils.createDoubleBuffer(1);
        DoubleBuffer mouseY = BufferUtils.createDoubleBuffer(1);

        GLFW.glfwGetCursorPos(window, mouseX, mouseY);

        double x = mouseX.get(0);
        double y = mouseY.get(0);

        // A free cursor wanders over the whole desk, and turning the camera after it would hand the view to whoever
        // is working next to the game: while the game would hold the cursor, it takes no motion at all, and the view
        // is aimed from outside (the look command). The menus read the position, which still follows.
        boolean ignoreMotion = freeMouse && mouseGrabbed;
        xposDelta = ignoreMotion ? 0 : x - this.xpos;
        yposDelta = ignoreMotion ? 0 : y - this.ypos;
        this.xpos = x;
        this.ypos = y;
    }

    @Override
    public Vector2i getPosition() {
        return new Vector2i((int) (xpos / this.uiScale), (int) (ypos / this.uiScale));
    }

    @Override
    public Vector2d getDelta() {
        return new Vector2d(xposDelta, yposDelta);
    }

    @Override
    public boolean isButtonDown(int button) {
        return buttonStates.contains(button);
    }

    @Override
    public boolean isVisible() {
        return !mouseGrabbed;
    }

    @Override
    public void setGrabbed(boolean newGrabbed) {
        if (newGrabbed != mouseGrabbed) {
            mouseGrabbed = newGrabbed;
            GLFW.glfwSetInputMode(GLFW.glfwGetCurrentContext(), GLFW.GLFW_CURSOR,
                    newGrabbed && !freeMouse ? GLFW.GLFW_CURSOR_DISABLED : GLFW.GLFW_CURSOR_NORMAL);
        }
    }

    @Override
    public Queue<MouseAction> getInputQueue() {
        Queue<MouseAction> mouseActions = Lists.newLinkedList();
        MouseAction action;
        while ((action = queue.poll()) != null) {
            mouseActions.add(action);
        }
        return mouseActions;
    }

    @Override
    public void propertyChange(PropertyChangeEvent evt) {
        if (evt.getPropertyName().equals(RenderingConfig.UI_SCALE)) {
            this.uiScale = this.renderingConfig.getUiScale() / 100f;
        }
    }

    private void focusCallback(long window, boolean isFocused) {
        // When the window is unfocused and focused again, the mouse coordinates
        // reported by GLFW change even if the mouse hasn't moved.
        // So when the window regains focus, we set the new position as the reference without generating deltas.
        if (isFocused) {
            DoubleBuffer mouseX = BufferUtils.createDoubleBuffer(1);
            DoubleBuffer mouseY = BufferUtils.createDoubleBuffer(1);

            GLFW.glfwGetCursorPos(window, mouseX, mouseY);

            xpos = mouseX.get(0);
            ypos = mouseY.get(0);
        }
    }

    private void mouseButtonCallback(long window, int button, int action, int mods) {
        ButtonState state;
        if (action == GLFW.GLFW_PRESS) {
            state = ButtonState.DOWN;
            buttonStates.add(button);
        } else if (action == GLFW.GLFW_RELEASE) {
            state = ButtonState.UP;
            buttonStates.remove(button);
        } else /*if (action == GLFW.GLFW_REPEAT)*/ {
            state = ButtonState.REPEAT;
        }
        MouseInput mouseInput = MouseInput.find(InputType.MOUSE_BUTTON, button);
        queue.offer(new MouseAction(mouseInput, state, getPosition()));
    }

    private void scrollCallback(long windows, double xoffset, double yoffset) {
        if (yoffset != 0.0) {
            int id = (yoffset > 0) ? 1 : -1;
            queue.offer(new MouseAction(InputType.MOUSE_WHEEL.getInput(id), 1, getPosition()));
        }
    }
}
