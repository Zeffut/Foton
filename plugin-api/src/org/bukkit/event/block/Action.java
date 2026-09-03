package org.bukkit.event.block;

public enum Action {
    LEFT_CLICK_BLOCK, RIGHT_CLICK_BLOCK, LEFT_CLICK_AIR, RIGHT_CLICK_AIR, PHYSICAL;

    public boolean isLeftClick() { return this == LEFT_CLICK_BLOCK || this == LEFT_CLICK_AIR; }
    public boolean isRightClick() { return this == RIGHT_CLICK_BLOCK || this == RIGHT_CLICK_AIR; }
}
