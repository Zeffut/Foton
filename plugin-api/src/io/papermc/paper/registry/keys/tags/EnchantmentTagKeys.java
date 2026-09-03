package io.papermc.paper.registry.keys.tags;

import io.papermc.paper.registry.RegistryKey;
import io.papermc.paper.registry.tag.TagKey;
import net.kyori.adventure.key.Key;
import org.bukkit.enchantments.Enchantment;

/** Standard vanilla enchantment tags. */
public final class EnchantmentTagKeys {
    private EnchantmentTagKeys() {}

    public static final TagKey<Enchantment> CURSE = of("curse");
    public static final TagKey<Enchantment> DOUBLE_TRADE_PRICE = of("double_trade_price");
    public static final TagKey<Enchantment> EXCLUSIVE_SET_ARMOR = of("exclusive_set/armor");
    public static final TagKey<Enchantment> EXCLUSIVE_SET_BOOTS = of("exclusive_set/boots");
    public static final TagKey<Enchantment> EXCLUSIVE_SET_BOW = of("exclusive_set/bow");
    public static final TagKey<Enchantment> EXCLUSIVE_SET_CROSSBOW = of("exclusive_set/crossbow");
    public static final TagKey<Enchantment> EXCLUSIVE_SET_DAMAGE = of("exclusive_set/damage");
    public static final TagKey<Enchantment> EXCLUSIVE_SET_MINING = of("exclusive_set/mining");
    public static final TagKey<Enchantment> EXCLUSIVE_SET_RIPTIDE = of("exclusive_set/riptide");
    public static final TagKey<Enchantment> IN_ENCHANTING_TABLE = of("in_enchanting_table");
    public static final TagKey<Enchantment> NON_TREASURE = of("non_treasure");
    public static final TagKey<Enchantment> ON_MOB_SPAWN_EQUIPMENT = of("on_mob_spawn_equipment");
    public static final TagKey<Enchantment> ON_RANDOM_LOOT = of("on_random_loot");
    public static final TagKey<Enchantment> ON_TRADED_EQUIPMENT = of("on_traded_equipment");
    public static final TagKey<Enchantment> PREVENTS_BEE_SPAWNS_WHEN_MINING = of("prevents_bee_spawns_when_mining");
    public static final TagKey<Enchantment> PREVENTS_DECORATED_POT_SHATTERING = of("prevents_decorated_pot_shattering");
    public static final TagKey<Enchantment> PREVENTS_ICE_MELTING = of("prevents_ice_melting");
    public static final TagKey<Enchantment> PREVENTS_INFESTED_SPAWNS = of("prevents_infested_spawns");
    public static final TagKey<Enchantment> SMELTS_LOOT = of("smelts_loot");
    public static final TagKey<Enchantment> TOOLTIP_ORDER = of("tooltip_order");
    public static final TagKey<Enchantment> TRADEABLE = of("tradeable");
    public static final TagKey<Enchantment> TREASURE = of("treasure");

    /** Paper's own signature, which a plugin may call. */
    public static TagKey<Enchantment> create(Key key) {
        return TagKey.create(RegistryKey.ENCHANTMENT, key);
    }

    /** The vanilla tags above, whose namespace is always minecraft. */
    private static TagKey<Enchantment> of(String path) {
        return create(Key.key("minecraft", path));
    }
}
