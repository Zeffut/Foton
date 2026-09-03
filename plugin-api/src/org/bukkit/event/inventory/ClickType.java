package org.bukkit.event.inventory;
public enum ClickType {
 LEFT, RIGHT, SHIFT_LEFT, SHIFT_RIGHT, WINDOW_BORDER_LEFT, WINDOW_BORDER_RIGHT,
 MIDDLE, NUMBER_KEY, DOUBLE_CLICK, DROP, CONTROL_DROP, CREATIVE,
 SWAP_OFFHAND, UNKNOWN;
 public boolean isLeftClick() { return this == LEFT || this == SHIFT_LEFT || this == WINDOW_BORDER_LEFT; }
 public boolean isRightClick() { return this == RIGHT || this == SHIFT_RIGHT || this == WINDOW_BORDER_RIGHT; }
 public boolean isShiftClick() { return this == SHIFT_LEFT || this == SHIFT_RIGHT; }
}
