package io.papermc.paper.registry.keys.tags;

import io.papermc.paper.registry.RegistryKey;
import io.papermc.paper.registry.tag.TagKey;
import net.kyori.adventure.key.Key;
import org.bukkit.enchantments.Enchantment;

/** Standard vanilla enchantment tags. */
public final class EnchantmentTagKeys {
    private EnchantmentTagKeys() {}

    public static final TagKey<Enchantment> CURSE = create("curse");
    public static final TagKey<Enchantment> DOUBLE_TRADE_PRICE = create("double_trade_price");
    public static final TagKey<Enchantment> EXCLUSIVE_SET_ARMOR = create("exclusive_set/armor");
    public static final TagKey<Enchantment> EXCLUSIVE_SET_BOOTS = create("exclusive_set/boots");
    public static final TagKey<Enchantment> EXCLUSIVE_SET_BOW = create("exclusive_set/bow");
    public static final TagKey<Enchantment> EXCLUSIVE_SET_CROSSBOW = create("exclusive_set/crossbow");
    public static final TagKey<Enchantment> EXCLUSIVE_SET_DAMAGE = create("exclusive_set/damage");
    public static final TagKey<Enchantment> EXCLUSIVE_SET_MINING = create("exclusive_set/mining");
    public static final TagKey<Enchantment> EXCLUSIVE_SET_RIPTIDE = create("exclusive_set/riptide");
    public static final TagKey<Enchantment> IN_ENCHANTING_TABLE = create("in_enchanting_table");
    public static final TagKey<Enchantment> NON_TREASURE = create("non_treasure");
    public static final TagKey<Enchantment> ON_MOB_SPAWN_EQUIPMENT = create("on_mob_spawn_equipment");
    public static final TagKey<Enchantment> ON_RANDOM_LOOT = create("on_random_loot");
    public static final TagKey<Enchantment> ON_TRADED_EQUIPMENT = create("on_traded_equipment");
    public static final TagKey<Enchantment> PREVENTS_BEE_SPAWNS_WHEN_MINING = create("prevents_bee_spawns_when_mining");
    public static final TagKey<Enchantment> PREVENTS_DECORATED_POT_SHATTERING = create("prevents_decorated_pot_shattering");
    public static final TagKey<Enchantment> PREVENTS_ICE_MELTING = create("prevents_ice_melting");
    public static final TagKey<Enchantment> PREVENTS_INFESTED_SPAWNS = create("prevents_infested_spawns");
    public static final TagKey<Enchantment> SMELTS_LOOT = create("smelts_loot");
    public static final TagKey<Enchantment> TOOLTIP_ORDER = create("tooltip_order");
    public static final TagKey<Enchantment> TRADEABLE = create("tradeable");
    public static final TagKey<Enchantment> TREASURE = create("treasure");

    public static TagKey<Enchantment> create(Key key) {
        return TagKey.create(RegistryKey.ENCHANTMENT, key);
    }
}
