package org.bukkit.entity;

/** Vanilla horse entity view. */
public interface Horse extends AbstractHorse {
    default boolean isTamed() { return foton.Native.entityIsTamed(((foton.FotonEntity) this).getUniqueId().toString()); }
    default void setTamed(boolean tamed) { foton.Native.setEntityTamed(((foton.FotonEntity) this).getUniqueId().toString(), tamed); }
    default org.bukkit.inventory.HorseInventory getInventory() { return null; }
    default boolean isCarryingChest() { return foton.Native.entityHasChest(((foton.FotonEntity) this).getUniqueId().toString()); }
    default void setCarryingChest(boolean carryingChest) { foton.Native.entitySetChest(((foton.FotonEntity) this).getUniqueId().toString(), carryingChest); }
    enum Color { WHITE, CREAMY, CHESTNUT, BROWN, BLACK, GRAY, DARK_BROWN }
    Color getColor();
    void setColor(Color color);
    enum Style { NONE, WHITE, WHITEFIELD, WHITEDOTS, WHITE_DOTS, BLACKDOTS;
        /** Modern spelling of Bukkit's historical BLACKDOTS constant. */
        public static final Style BLACK_DOTS = BLACKDOTS;
    }
    Style getStyle();
    void setStyle(Style style);
}
