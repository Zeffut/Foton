package org.bukkit.inventory;

import org.bukkit.Keyed;
import org.bukkit.Material;

/**
 * Typed registry entry for an item, the item-side counterpart of
 * {@link org.bukkit.block.BlockType}.
 *
 * <p>Paper's own interface carries a constant per item and a large accessor
 * surface. Neither is present here yet: no plugin in the measured corpus calls
 * an {@code ItemType} member, and the type is needed today only so that
 * {@code RegistryKey.ITEM} and the registry sets built over it resolve. It
 * grows in the order {@code dev/plugin-api-usage.json} ranks, like the rest of
 * this API.</p>
 */
public interface ItemType extends Keyed {
    /** The material this item type denotes. */
    Material asMaterial();
}
