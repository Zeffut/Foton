package io.papermc.paper.registry.keys.tags;

import io.papermc.paper.registry.RegistryKey;
import io.papermc.paper.registry.tag.TagKey;
import net.kyori.adventure.key.Key;
import org.bukkit.inventory.ItemType;

/** Standard vanilla item tags. */
public final class ItemTypeTagKeys {
    private ItemTypeTagKeys() {}

    public static final TagKey<ItemType> PICKAXES = of("pickaxes");
    public static final TagKey<ItemType> AXES = of("axes");
    public static final TagKey<ItemType> SHOVELS = of("shovels");
    public static final TagKey<ItemType> HOES = of("hoes");
    public static final TagKey<ItemType> SWORDS = of("swords");
    public static final TagKey<ItemType> ENCHANTABLE_MINING = of("enchantable/mining");
    public static final TagKey<ItemType> ENCHANTABLE_WEAPON = of("enchantable/weapon");
    public static final TagKey<ItemType> ENCHANTABLE_DURABILITY = of("enchantable/durability");

    /** Paper's own signature, which a plugin may call. */
    public static TagKey<ItemType> create(Key key) {
        return TagKey.create(RegistryKey.ITEM, key);
    }

    /** The vanilla tags above, whose namespace is always minecraft. */
    private static TagKey<ItemType> of(String path) {
        return create(Key.key("minecraft", path));
    }
}
