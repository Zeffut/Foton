package org.bukkit.inventory;

/** Equipment groups used by attribute modifiers. */
public enum EquipmentSlotGroup {
    ANY, HAND, MAINHAND, OFFHAND, ARMOR, HEAD, CHEST, LEGS, FEET, BODY

    ;
    public static EquipmentSlotGroup getByName(String name) {
        if (name == null) return null;
        for (EquipmentSlotGroup value : values())
            if (value.name().equalsIgnoreCase(name)) return value;
        return null;
    }
}
