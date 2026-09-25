package com.destroystokyo.paper.event.player;

import static org.bukkit.Material.*;

import java.util.Set;
import org.bukkit.Material;
import org.bukkit.entity.Player;
import org.bukkit.event.HandlerList;
import org.bukkit.event.player.PlayerEvent;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.ItemStack;

/** A piece of a player's armor changed, however it happened. Fired once the
 * change is made; not cancellable. */
public class PlayerArmorChangeEvent extends PlayerEvent {
    private static final HandlerList HANDLERS = new HandlerList();
    private final SlotType slotType;
    private final ItemStack oldItem;
    private final ItemStack newItem;

    public PlayerArmorChangeEvent(Player player, SlotType slotType, ItemStack oldItem, ItemStack newItem) {
        super(player);
        this.slotType = slotType;
        this.oldItem = oldItem;
        this.newItem = newItem;
    }

    @Deprecated
    public SlotType getSlotType() { return slotType; }

    public EquipmentSlot getSlot() {
        return switch (slotType) {
            case HEAD -> EquipmentSlot.HEAD;
            case CHEST -> EquipmentSlot.CHEST;
            case LEGS -> EquipmentSlot.LEGS;
            case FEET -> EquipmentSlot.FEET;
        };
    }

    public ItemStack getOldItem() { return oldItem; }
    public ItemStack getNewItem() { return newItem; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }

    /** The four armor slots, each with the materials Paper lists as fitting it. */
    @Deprecated
    public enum SlotType {
        HEAD(COPPER_HELMET, NETHERITE_HELMET, DIAMOND_HELMET, GOLDEN_HELMET, IRON_HELMET,
            CHAINMAIL_HELMET, LEATHER_HELMET, CARVED_PUMPKIN, PLAYER_HEAD, SKELETON_SKULL,
            ZOMBIE_HEAD, CREEPER_HEAD, WITHER_SKELETON_SKULL, TURTLE_HELMET, DRAGON_HEAD,
            PIGLIN_HEAD),
        CHEST(COPPER_CHESTPLATE, NETHERITE_CHESTPLATE, DIAMOND_CHESTPLATE, GOLDEN_CHESTPLATE,
            IRON_CHESTPLATE, CHAINMAIL_CHESTPLATE, LEATHER_CHESTPLATE, ELYTRA),
        LEGS(COPPER_LEGGINGS, NETHERITE_LEGGINGS, DIAMOND_LEGGINGS, GOLDEN_LEGGINGS,
            IRON_LEGGINGS, CHAINMAIL_LEGGINGS, LEATHER_LEGGINGS),
        FEET(COPPER_BOOTS, NETHERITE_BOOTS, DIAMOND_BOOTS, GOLDEN_BOOTS, IRON_BOOTS,
            CHAINMAIL_BOOTS, LEATHER_BOOTS);

        private final Set<Material> types;

        SlotType(Material... types) { this.types = Set.of(types); }

        public Set<Material> getTypes() { return types; }

        public static SlotType getByMaterial(Material material) {
            for (SlotType slotType : values()) {
                if (slotType.getTypes().contains(material)) return slotType;
            }
            return null;
        }

        public static boolean isEquipable(Material material) { return getByMaterial(material) != null; }
    }
}
