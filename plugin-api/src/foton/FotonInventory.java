package foton;

import org.bukkit.Material;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.PlayerInventory;
import org.bukkit.inventory.InventoryHolder;

/** A player's inventory, read and written across JNI.
 *
 * Every slot is a separate crossing, which is slower than one call for the
 * whole thing and is what keeps the contract honest: nothing here holds a
 * reference into the live inventory, so a plugin cannot change a chest from
 * its own thread while the tick is reading it.
 *
 * A slot crosses as a string -- `minecraft:diamond_sword 3` -- rather than as
 * an object. Building a Java object from Rust means calling a constructor by
 * signature, and a signature that drifts is a NoSuchMethodError at the worst
 * possible moment; a string that fails to parse is an empty slot.
 */
public final class FotonInventory implements PlayerInventory {
    /** Vanilla's player inventory: 36 storage slots, 4 armor, 1 offhand. */
    private static final int SIZE = 41;
    private static final int ARMOR = 36;
    private static final int OFFHAND = 40;

    private final String owner;

    FotonInventory(String owner) {
        this.owner = owner;
    }

    @Override
    public InventoryHolder getHolder() {
        try {
            return new FotonPlayer(java.util.UUID.fromString(owner));
        } catch (IllegalArgumentException error) {
            return null;
        }
    }

    @Override
    public int getSize() {
        return SIZE;
    }

    @Override public org.bukkit.event.inventory.InventoryType getType() {
        return org.bukkit.event.inventory.InventoryType.PLAYER;
    }

    @Override
    public ItemStack getItem(int slot) {
        return decode(Native.inventorySlot(owner, slot));
    }

    @Override
    public void setItem(int slot, ItemStack item) {
        Native.setInventorySlot(owner, slot, encode(item));
    }

    @Override
    public java.util.HashMap<Integer, ItemStack> addItem(ItemStack... items) {
        java.util.HashMap<Integer, ItemStack> leftovers = new java.util.HashMap<>();
        if (items == null) return leftovers;
        for (int index = 0; index < items.length; index++) {
            ItemStack incoming = items[index] == null ? null : items[index].clone();
            if (incoming == null || incoming.getType().isAir() || incoming.getAmount() <= 0) continue;
            for (int slot = 0; slot < getSize() && incoming.getAmount() > 0; slot++) {
                ItemStack current = getItem(slot);
                if (current != null && current.isSimilar(incoming)) {
                    int space = current.getMaxStackSize() - current.getAmount();
                    if (space > 0) {
                        int moved = Math.min(space, incoming.getAmount());
                        current.setAmount(current.getAmount() + moved);
                        incoming.setAmount(incoming.getAmount() - moved);
                        setItem(slot, current);
                    }
                }
            }
            for (int slot = 0; slot < getSize() && incoming.getAmount() > 0; slot++) {
                ItemStack current = getItem(slot);
                if (current == null || current.getType().isAir() || current.getAmount() <= 0) {
                    int moved = Math.min(incoming.getMaxStackSize(), incoming.getAmount());
                    ItemStack placed = incoming.clone();
                    placed.setAmount(moved);
                    setItem(slot, placed);
                    incoming.setAmount(incoming.getAmount() - moved);
                }
            }
            if (incoming.getAmount() > 0) leftovers.put(index, incoming);
        }
        return leftovers;
    }

    @Override
    public ItemStack[] getContents() {
        ItemStack[] contents = new ItemStack[SIZE];
        for (int slot = 0; slot < SIZE; slot++) {
            contents[slot] = getItem(slot);
        }
        return contents;
    }

    @Override
    public void setContents(ItemStack[] items) {
        for (int slot = 0; slot < SIZE; slot++) {
            setItem(slot, items != null && slot < items.length ? items[slot] : null);
        }
    }

    @Override
    public boolean contains(Material material) {
        return first(material) >= 0;
    }

    @Override
    public int first(Material material) {
        if (material == null) {
            return -1;
        }
        for (int slot = 0; slot < SIZE; slot++) {
            ItemStack item = getItem(slot);
            if (item != null && item.getType() == material) {
                return slot;
            }
        }
        return -1;
    }

    @Override
    public void clear() {
        for (int slot = 0; slot < SIZE; slot++) {
            clear(slot);
        }
    }

    @Override
    public void clear(int slot) {
        Native.setInventorySlot(owner, slot, "");
    }

    @Override
    public ItemStack getItemInMainHand() {
        return getItem(getHeldItemSlot());
    }

    @Override
    public void setItemInMainHand(ItemStack item) {
        setItem(getHeldItemSlot(), item);
    }

    @Override
    public ItemStack getItemInHand() {
        return getItemInMainHand();
    }

    @Override
    public void setItemInHand(ItemStack item) {
        setItemInMainHand(item);
    }

    @Override
    public ItemStack getItemInOffHand() {
        return getItem(OFFHAND);
    }

    @Override
    public void setItemInOffHand(ItemStack item) {
        setItem(OFFHAND, item);
    }

    // Armor runs boots, leggings, chestplate, helmet from slot 36 upward,
    // which is the order the protocol uses and the reverse of how it reads.
    @Override
    public ItemStack[] getArmorContents() {
        return new ItemStack[] { getBoots(), getLeggings(), getChestplate(), getHelmet() };
    }

    @Override public ItemStack[] getStorageContents() {
        ItemStack[] result = new ItemStack[ARMOR];
        for (int slot = 0; slot < result.length; slot++) result[slot] = getItem(slot);
        return result;
    }
    @Override public void setStorageContents(ItemStack[] contents) {
        for (int slot = 0; slot < ARMOR; slot++) setItem(slot, contents != null && slot < contents.length ? contents[slot] : null);
    }

    @Override
    public ItemStack getHelmet() {
        return getItem(ARMOR + 3);
    }
    @Override public void setHelmet(ItemStack item) { setItem(ARMOR + 3, item); }

    @Override
    public ItemStack getChestplate() {
        return getItem(ARMOR + 2);
    }
    @Override public void setChestplate(ItemStack item) { setItem(ARMOR + 2, item); }

    @Override
    public ItemStack getLeggings() {
        return getItem(ARMOR + 1);
    }
    @Override public void setLeggings(ItemStack item) { setItem(ARMOR + 1, item); }

    @Override
    public ItemStack getBoots() {
        return getItem(ARMOR);
    }
    @Override public void setBoots(ItemStack item) { setItem(ARMOR, item); }

    @Override
    public int getHeldItemSlot() {
        return Math.max(0, Native.heldSlot(owner));
    }

    /** Reads `minecraft:diamond_sword 3`. Anything else is an empty slot. */
    public static ItemStack decode(String text) {
        if (text == null || text.isEmpty()) {
            return null;
        }
        String[] encoded = text.split("\\u001d", -1);
        text = encoded[0];
        int space = text.lastIndexOf(' ');
        String name = space < 0 ? text : text.substring(0, space);
        int amount = 1;
        if (space >= 0) {
            try {
                amount = Integer.parseInt(text.substring(space + 1));
            } catch (NumberFormatException notANumber) {
                return null;
            }
        }
        Material material = Material.matchMaterial(name);
        if (material == null || material.isAir() || amount <= 0) {
            return null;
        }
        ItemStack result = new ItemStack(material, amount);
        for (int i = 1; i < encoded.length; i++) if (encoded[i].startsWith("damage=")) {
            try { result.setDurability(Short.parseShort(encoded[i].substring(7))); } catch (NumberFormatException ignored) { }
        }
        for (String field : encoded) if (field.startsWith("nbthex=")) {
            String hex = field.substring(7); java.io.ByteArrayOutputStream raw = new java.io.ByteArrayOutputStream();
            for (int i = 0; i + 1 < hex.length(); i += 2) try { raw.write(Integer.parseInt(hex.substring(i, i + 2), 16)); } catch (NumberFormatException ignored) { raw.reset(); break; }
            if (raw.size() > 0) result.setOpaqueNbt(new String(raw.toByteArray(), java.nio.charset.StandardCharsets.UTF_8));
        }
        if (encoded.length > 1 && result.getItemMeta() instanceof org.bukkit.inventory.meta.ItemMeta meta) {
            for (String field : encoded) if (field.equals("unbreakable")) meta.setUnbreakable(true);
            for (String field : encoded) if (field.equals("hidetooltip")) meta.setHideTooltip(true);
            for (String field : encoded) if (field.startsWith("tooltipstylehex=")) {
                String style = new String(hexDecode(field.substring(16)), java.nio.charset.StandardCharsets.UTF_8);
                org.bukkit.NamespacedKey key = org.bukkit.NamespacedKey.fromString(style);
                if (key != null) meta.setTooltipStyle(key);
            }
            for (String field : encoded) if (field.startsWith("itemmodelhex=")) {
                String model = new String(hexDecode(field.substring(13)), java.nio.charset.StandardCharsets.UTF_8);
                org.bukkit.NamespacedKey key = org.bukkit.NamespacedKey.fromString(model);
                if (key != null) meta.setItemModel(key);
            }
            for (String field : encoded) if (field.startsWith("model=")) {
                try { meta.setCustomModelData(Integer.parseInt(field.substring(6))); }
                catch (NumberFormatException ignored) { }
            }
            org.bukkit.inventory.meta.components.CustomModelDataComponent model = meta.getCustomModelDataComponent();
            java.util.ArrayList<Float> floats = new java.util.ArrayList<>();
            java.util.ArrayList<Boolean> flags = new java.util.ArrayList<>();
            java.util.ArrayList<String> strings = new java.util.ArrayList<>();
            java.util.ArrayList<org.bukkit.Color> colors = new java.util.ArrayList<>();
            for (String field : encoded) {
                try {
                    if (field.startsWith("modelfloat=")) floats.add(Float.parseFloat(field.substring(11)));
                    else if (field.startsWith("modelflag=")) flags.add(Boolean.parseBoolean(field.substring(10)));
                    else if (field.startsWith("modelstrhex=")) strings.add(new String(hexDecode(field.substring(12)), java.nio.charset.StandardCharsets.UTF_8));
                    else if (field.startsWith("modelcolor=")) colors.add(org.bukkit.Color.fromRGB(Integer.parseInt(field.substring(11))));
                } catch (NumberFormatException ignored) { }
            }
            if (!floats.isEmpty() || !flags.isEmpty() || !strings.isEmpty() || !colors.isEmpty()) {
                model.setFloats(floats); model.setFlags(flags); model.setStrings(strings); model.setColors(colors);
                meta.setCustomModelDataComponent(model);
            }
            for (String field : encoded) if (field.startsWith("namehex=")) {
                String hex = field.substring(8);
                java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
                for (int i = 0; i + 1 < hex.length(); i += 2) {
                    try { bytes.write(Integer.parseInt(hex.substring(i, i + 2), 16)); }
                    catch (NumberFormatException ignored) { bytes.reset(); break; }
                }
                if (bytes.size() > 0) meta.setDisplayName(new String(bytes.toByteArray(), java.nio.charset.StandardCharsets.UTF_8));
            }
            java.util.ArrayList<String> lore = new java.util.ArrayList<>();
            for (String field : encoded) if (field.startsWith("lorehex=")) lore.add(new String(hexDecode(field.substring(8)), java.nio.charset.StandardCharsets.UTF_8));
            if (!lore.isEmpty()) meta.setLore(lore);
            for (String field : encoded) if (field.startsWith("enchhex=") || field.startsWith("storedenchhex=")) {
                boolean stored = field.startsWith("storedenchhex=");
                String payload = field.substring(stored ? 14 : 8);
                int separator = payload.lastIndexOf(':');
                if (separator <= 0) continue;
                try {
                    String enchantmentName = new String(hexDecode(payload.substring(0, separator)), java.nio.charset.StandardCharsets.UTF_8);
                    int namespace = enchantmentName.lastIndexOf(':');
                    if (namespace >= 0) enchantmentName = enchantmentName.substring(namespace + 1);
                    int level = Integer.parseInt(payload.substring(separator + 1));
                    org.bukkit.enchantments.Enchantment enchantment = org.bukkit.enchantments.Enchantment.getByName(enchantmentName);
                    if (enchantment != null) {
                        if (stored && meta instanceof org.bukkit.inventory.meta.EnchantmentStorageMeta storage) storage.addStoredEnchant(enchantment, level, true);
                        else meta.addEnchant(enchantment, level, true);
                    }
                } catch (NumberFormatException ignored) { }
            }
            result.setItemMeta(meta);
        }
        if (result.getItemMeta() instanceof org.bukkit.inventory.meta.PotionMeta meta) {
            for (String encodedField : encoded) {
                if (encodedField.startsWith("damage=") || encodedField.startsWith("namehex=")
                        || encodedField.startsWith("lorehex=") || encodedField.startsWith("enchhex=")
                        || encodedField.startsWith("storedenchhex=")) continue;
                for (String effect : encodedField.split(";")) {
                    String[] fields = effect.split(",", -1);
                    if (fields.length < 3) continue;
                    org.bukkit.potion.PotionEffectType type = org.bukkit.potion.PotionEffectType.getByName(fields[0]);
                    try { if (type != null) meta.addCustomEffect(new org.bukkit.potion.PotionEffect(type, Integer.parseInt(fields[1]), Integer.parseInt(fields[2])), true); }
                    catch (NumberFormatException ignored) { }
                }
            }
            result.setItemMeta(meta);
        }
        return result;
    }

    private static String hexEncode(String value) {
        StringBuilder hex = new StringBuilder();
        for (byte byteValue : value.getBytes(java.nio.charset.StandardCharsets.UTF_8))
            hex.append(String.format("%02x", byteValue & 0xff));
        return hex.toString();
    }

    private static byte[] hexDecode(String value) {
        java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
        for (int i = 0; i + 1 < value.length(); i += 2) {
            try { bytes.write(Integer.parseInt(value.substring(i, i + 2), 16)); }
            catch (NumberFormatException ignored) { return new byte[0]; }
        }
        return bytes.toByteArray();
    }

    /** Writes what decode reads. An empty stack is an empty string. */
    public static String encode(ItemStack item) {
        if (item == null || item.getType().isAir() || item.getAmount() <= 0) {
            return "";
        }
        String value = "minecraft:" + item.getType().getKeyName() + " " + item.getAmount();
        if (item.getOpaqueNbt() != null && !item.getOpaqueNbt().isEmpty()) value += "\u001dnbthex=" + hexEncode(item.getOpaqueNbt());
        if (item.getDurability() != 0) value += "\u001ddamage=" + item.getDurability();
        if (item.hasItemMeta() && item.getItemMeta().isUnbreakable()) value += "\u001dunbreakable";
        if (item.hasItemMeta() && item.getItemMeta().isHideTooltip()) value += "\u001dhidetooltip";
        if (item.hasItemMeta() && item.getItemMeta().hasItemModel())
            value += "\u001ditemmodelhex=" + hexEncode(item.getItemMeta().getItemModel().toString());
        if (item.hasItemMeta() && item.getItemMeta().hasTooltipStyle())
            value += "\u001dtooltipstylehex=" + hexEncode(item.getItemMeta().getTooltipStyle().toString());
        if (item.hasItemMeta() && item.getItemMeta().hasCustomModelData())
            value += "\u001dmodel=" + item.getItemMeta().getCustomModelData();
        if (item.hasItemMeta()) {
            org.bukkit.inventory.meta.components.CustomModelDataComponent model = item.getItemMeta().getCustomModelDataComponent();
            for (Float valuePart : model.getFloats()) value += "\u001dmodelfloat=" + valuePart;
            for (Boolean valuePart : model.getFlags()) value += "\u001dmodelflag=" + valuePart;
            for (String valuePart : model.getStrings()) {
                StringBuilder hex = new StringBuilder();
                for (byte byteValue : valuePart.getBytes(java.nio.charset.StandardCharsets.UTF_8)) hex.append(String.format("%02x", byteValue & 0xff));
                value += "\u001dmodelstrhex=" + hex;
            }
            for (org.bukkit.Color valuePart : model.getColors()) value += "\u001dmodelcolor=" + valuePart.asRGB();
        }
        if (item.hasItemMeta() && item.getItemMeta().hasDisplayName()) {
            String name = item.getItemMeta().getDisplayName();
            StringBuilder hex = new StringBuilder();
            for (byte byteValue : name.getBytes(java.nio.charset.StandardCharsets.UTF_8)) hex.append(String.format("%02x", byteValue & 0xff));
            value += "\u001dnamehex=" + hex;
        }
        if (item.hasItemMeta()) {
            for (java.util.Map.Entry<org.bukkit.enchantments.Enchantment, Integer> entry : item.getItemMeta().getEnchants().entrySet()) {
                StringBuilder hex = new StringBuilder();
                for (byte byteValue : entry.getKey().getKey().toString().getBytes(java.nio.charset.StandardCharsets.UTF_8)) hex.append(String.format("%02x", byteValue & 0xff));
                value += "\u001denchhex=" + hex + ":" + entry.getValue();
            }
            if (item.getItemMeta() instanceof org.bukkit.inventory.meta.EnchantmentStorageMeta storage)
                for (java.util.Map.Entry<org.bukkit.enchantments.Enchantment, Integer> entry : storage.getStoredEnchants().entrySet()) {
                    StringBuilder hex = new StringBuilder();
                    for (byte byteValue : entry.getKey().getKey().toString().getBytes(java.nio.charset.StandardCharsets.UTF_8)) hex.append(String.format("%02x", byteValue & 0xff));
                    value += "\u001dstoredenchhex=" + hex + ":" + entry.getValue();
                }
        }
        if (item.hasItemMeta() && item.getItemMeta().hasLore()) {
            for (String line : item.getItemMeta().getLore()) {
                StringBuilder hex = new StringBuilder();
                for (byte byteValue : line.getBytes(java.nio.charset.StandardCharsets.UTF_8)) hex.append(String.format("%02x", byteValue & 0xff));
                value += "\u001dlorehex=" + hex;
            }
        }
        if (item.getItemMeta() instanceof org.bukkit.inventory.meta.PotionMeta meta && !meta.getCustomEffects().isEmpty()) {
            StringBuilder effects = new StringBuilder("\u001d");
            for (org.bukkit.potion.PotionEffect effect : meta.getCustomEffects()) effects.append(effect.getType().getName()).append(',').append(effect.getDuration()).append(',').append(effect.getAmplifier()).append(';');
            value += effects;
        }
        return value;
    }
}
