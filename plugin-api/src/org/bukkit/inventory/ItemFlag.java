package org.bukkit.inventory;

/** Hides a part of an item's tooltip from the client.
 *
 * <p>Since 1.21.5 a flag is not stored anywhere: it names data components in
 * the item's {@code tooltip_display}, and an item has the flag exactly when
 * all of them are hidden (see
 * {@link org.bukkit.inventory.meta.SimpleItemMeta}).</p>
 */
public enum ItemFlag {
    HIDE_ENCHANTS,
    HIDE_ATTRIBUTES,
    HIDE_UNBREAKABLE,
    HIDE_DESTROYS,
    HIDE_PLACED_ON,
    /** The item-specific lines vanilla used to hide as one: potion effects,
     * book author, banner patterns, goat horn sound, shulker contents... */
    @Deprecated
    HIDE_ADDITIONAL_TOOLTIP,
    HIDE_DYE,
    HIDE_ARMOR_TRIM,
    HIDE_STORED_ENCHANTS,
}
