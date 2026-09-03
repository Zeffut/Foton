package org.bukkit.inventory;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.EOFException;
import java.io.IOException;
import java.util.List;

import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.inventory.meta.Damageable;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.inventory.meta.SimpleItemMeta;

/** A stack of one material.
 *
 * The most-constructed type in the corpus: thirty-two of the fifty-nine
 * plugins surveyed call `new ItemStack`.
 *
 * Mutable, like Bukkit's, and a plugin that hands one to the server is handing
 * over a description rather than a live reference into a chest -- reading an
 * inventory gives copies, and writing one takes them.
 */
public class ItemStack implements Cloneable {
    private Material type;
    private int amount;
    private short durability;
    private ItemMeta meta;
    /** Opaque NBT retained by Foton for plugin round-tripping. */
    private String opaqueNbt;
    private final java.util.IdentityHashMap<io.papermc.paper.datacomponent.DataComponentType<?>, Object> dataComponents = new java.util.IdentityHashMap<>();

    public ItemStack(Material type) {
        this(type, 1);
    }

    public ItemStack(Material type, int amount, short durability) { this(type, amount); this.durability = durability; }

    /** Legacy constructor retained for ViaVersion and other pre-flattening integrations. */
    @Deprecated
    public ItemStack(Material type, int amount, short durability, Byte data) {
        this(type, amount, durability);
        if (data != null) this.durability = data;
    }

    public ItemStack(Material type, int amount) {
        this.type = type == null ? Material.AIR : type;
        this.amount = amount;
    }

    public Material getType() {
        return type;
    }

    /** Changing the type to air empties the stack, which is what Bukkit does
     * and what a plugin clearing a slot relies on. */
    public void setType(Material type) {
        this.type = type == null ? Material.AIR : type;
        if (this.type.isAir()) {
            this.amount = 0;
            this.meta = null;
        }
    }

    public int getAmount() {
        return amount;
    }

    public void setAmount(int amount) {
        this.amount = amount;
    }

    @Deprecated public org.bukkit.material.MaterialData getData() { return new org.bukkit.material.MaterialData(type, (byte) durability); }

    public short getDurability() { return durability; }

    public String getOpaqueNbt() { return opaqueNbt; }
    public void setOpaqueNbt(String value) { opaqueNbt = value; }

    /** Stores a typed Paper data component on this stack. */
    public <T> void setData(io.papermc.paper.datacomponent.DataComponentType.Valued<T> type, T value) {
        if (type == null) throw new IllegalArgumentException("type");
        if (value == null) dataComponents.remove(type); else dataComponents.put(type, value);
        if (type == io.papermc.paper.datacomponent.DataComponentTypes.CUSTOM_MODEL_DATA && value instanceof io.papermc.paper.datacomponent.item.CustomModelData model) {
            ItemMeta copy = getItemMeta();
            org.bukkit.inventory.meta.components.CustomModelDataComponent target = copy.getCustomModelDataComponent();
            target.setFloats(model.floats()); target.setFlags(model.flags()); target.setStrings(model.strings()); java.util.ArrayList<org.bukkit.Color> colors = new java.util.ArrayList<>(); for (Integer color : model.colors()) colors.add(org.bukkit.Color.fromRGB(color)); target.setColors(colors);
            copy.setCustomModelDataComponent(target); setItemMeta(copy);
        }
    }
    @SuppressWarnings("unchecked")
    public <T> T getData(io.papermc.paper.datacomponent.DataComponentType<T> type) { return (T) dataComponents.get(type); }

    /** Legacy numeric item id; modern materials intentionally do not expose one. */
    @Deprecated
    public int getTypeId() { return type.getId(); }

    public void setDurability(short durability) { this.durability = durability; }

    public void addEnchantment(org.bukkit.enchantments.Enchantment enchantment, int level) { if (enchantment == null || level <= 0 || level > enchantment.getMaxLevel()) throw new IllegalArgumentException("Invalid enchantment level"); ItemMeta copy=getItemMeta(); if(copy.addEnchant(enchantment, level, false)) setItemMeta(copy); }

    public void addUnsafeEnchantment(org.bukkit.enchantments.Enchantment enchantment, int level) {
        ItemMeta copy = getItemMeta(); boolean changed = copy.addEnchant(enchantment, level, true); if (changed) setItemMeta(copy);
    }

    public int removeEnchantment(org.bukkit.enchantments.Enchantment enchantment) { ItemMeta copy = getItemMeta(); int old = copy.getEnchantLevel(enchantment); if (copy.removeEnchant(enchantment)) setItemMeta(copy); return old; }

    public java.util.Map<org.bukkit.enchantments.Enchantment, Integer> getEnchantments() { return getItemMeta().getEnchants(); }

    public java.util.Map<String, Object> serialize() { java.util.Map<String,Object> values=new java.util.LinkedHashMap<>(); values.put("type", type.getKeyName()); values.put("amount", amount); if (durability != 0) values.put("durability", durability); if (meta != null) { java.util.Map<String,Object> m=new java.util.LinkedHashMap<>(); if(meta.hasDisplayName())m.put("display-name",meta.getDisplayName()); if(meta.hasLore())m.put("lore",meta.getLore()); if(meta.hasCustomModelData())m.put("custom-model-data",meta.getCustomModelData()); if(meta.isUnbreakable())m.put("Unbreakable",true); values.put("meta",m); } return values; }

    public static ItemStack deserialize(java.util.Map<String, Object> values) {
        if (values == null) throw new IllegalArgumentException("values");
        Object raw = values.get("type"); Material material = raw instanceof Material m ? m : Material.matchMaterial(String.valueOf(raw));
        if (material == null) throw new IllegalArgumentException("Unknown material: " + raw);
        int count = values.get("amount") instanceof Number n ? n.intValue() : 1;
        short damage = values.get("durability") instanceof Number n ? n.shortValue() : 0;
        ItemStack stack = new ItemStack(material, count); stack.setDurability(damage); Object rawMeta=values.get("meta"); if(rawMeta instanceof java.util.Map<?,?> m){ ItemMeta meta=stack.getItemMeta(); Object n=m.get("display-name"); if(n instanceof String v)meta.setDisplayName(v); Object l=m.get("lore"); if(l instanceof java.util.List<?> v){ java.util.ArrayList<String> lines=new java.util.ArrayList<>(); for(Object x:v)if(x instanceof String z)lines.add(z); meta.setLore(lines); } Object cmd=m.get("custom-model-data"); if(cmd instanceof Number v)meta.setCustomModelData(v.intValue()); if(Boolean.TRUE.equals(m.get("Unbreakable")))meta.setUnbreakable(true); stack.setItemMeta(meta); } return stack;
    }

    /**
     * Serializes this stack to Foton's versioned, deterministic binary format.
     *
     * <p>This is intentionally an API-level format, not Minecraft's network/NBT
     * format. It preserves the fields represented by this Bukkit implementation:
     * type, amount, durability, common metadata, enchantments, and item flags.
     * Unknown or future metadata must use a newer format version.</p>
     *
     * @return a self-contained binary representation
     * @throws IllegalStateException if the stack cannot be encoded
     */
    public byte[] serializeAsBytes() {
        try {
            ByteArrayOutputStream bytes = new ByteArrayOutputStream();
            DataOutputStream out = new DataOutputStream(bytes);
            out.writeInt(0x46544F4E); // "FTON"
            out.writeByte(1);
            out.writeUTF(type.getKeyName());
            out.writeInt(amount);
            out.writeShort(durability);

            ItemMeta value = meta;
            out.writeBoolean(value != null);
            if (value != null) {
                writeNullableString(out, value.hasDisplayName() ? value.getDisplayName() : null);
                List<String> lore = value.getLore();
                out.writeInt(lore == null ? -1 : lore.size());
                if (lore != null) for (String line : lore) out.writeUTF(line == null ? "" : line);
                out.writeBoolean(value.hasCustomModelData());
                if (value.hasCustomModelData()) out.writeInt(value.getCustomModelData());
                out.writeBoolean(value.isUnbreakable());

                out.writeBoolean(value instanceof Damageable);
                if (value instanceof Damageable damageable) out.writeInt(damageable.getDamage());

                java.util.Map<org.bukkit.enchantments.Enchantment, Integer> enchants = value.getEnchants();
                java.util.List<org.bukkit.enchantments.Enchantment> sorted = new java.util.ArrayList<>(enchants.keySet());
                sorted.sort(java.util.Comparator.comparing(e -> e.getKey().toString()));
                out.writeInt(sorted.size());
                for (org.bukkit.enchantments.Enchantment enchantment : sorted) {
                    out.writeUTF(enchantment.getKey().toString());
                    out.writeInt(enchants.get(enchantment));
                }

                java.util.List<String> flags = new java.util.ArrayList<>();
                for (org.bukkit.inventory.ItemFlag flag : value.getItemFlags()) flags.add(flag.name());
                flags.sort(String::compareTo);
                out.writeInt(flags.size());
                for (String flag : flags) out.writeUTF(flag);
            }
            out.flush();
            return bytes.toByteArray();
        } catch (IOException impossible) {
            throw new IllegalStateException("could not serialize item stack", impossible);
        }
    }

    /**
     * Reads a stack produced by {@link #serializeAsBytes()}.
     *
     * @param bytes the encoded stack
     * @return a newly allocated stack
     * @throws IllegalArgumentException for malformed, truncated, unsupported,
     *         or trailing data
     */
    public static ItemStack deserializeBytes(byte[] bytes) {
        if (bytes == null) throw new IllegalArgumentException("bytes");
        try {
            DataInputStream in = new DataInputStream(new ByteArrayInputStream(bytes));
            if (in.readInt() != 0x46544F4E) throw new IllegalArgumentException("invalid ItemStack binary header");
            if (in.readUnsignedByte() != 1) throw new IllegalArgumentException("unsupported ItemStack binary version");
            Material material = Material.matchMaterial(in.readUTF());
            if (material == null) throw new IllegalArgumentException("unknown material in ItemStack binary data");
            ItemStack stack = new ItemStack(material, in.readInt());
            stack.setDurability(in.readShort());
            if (in.readBoolean()) {
                ItemMeta value = stack.getItemMeta();
                String displayName = readNullableString(in);
                if (displayName != null) value.setDisplayName(displayName);
                int loreSize = in.readInt();
                if (loreSize < -1 || loreSize > 1024) throw new IllegalArgumentException("invalid lore size");
                if (loreSize >= 0) {
                    java.util.ArrayList<String> lore = new java.util.ArrayList<>(loreSize);
                    for (int i = 0; i < loreSize; i++) lore.add(in.readUTF());
                    value.setLore(lore);
                }
                if (in.readBoolean()) value.setCustomModelData(in.readInt());
                value.setUnbreakable(in.readBoolean());
                if (in.readBoolean() && value instanceof Damageable damageable) damageable.setDamage(in.readInt());

                int enchantmentCount = in.readInt();
                if (enchantmentCount < 0 || enchantmentCount > 256) throw new IllegalArgumentException("invalid enchantment count");
                for (int i = 0; i < enchantmentCount; i++) {
                    org.bukkit.enchantments.Enchantment enchantment =
                        org.bukkit.enchantments.Enchantment.getByKey(NamespacedKey.fromString(in.readUTF()));
                    int level = in.readInt();
                    if (enchantment == null || level <= 0) throw new IllegalArgumentException("invalid enchantment");
                    value.addEnchant(enchantment, level, true);
                }

                int flagCount = in.readInt();
                if (flagCount < 0 || flagCount > 256) throw new IllegalArgumentException("invalid item flag count");
                for (int i = 0; i < flagCount; i++) {
                    try {
                        value.addItemFlags(org.bukkit.inventory.ItemFlag.valueOf(in.readUTF()));
                    } catch (IllegalArgumentException invalidFlag) {
                        throw new IllegalArgumentException("invalid item flag", invalidFlag);
                    }
                }
                stack.setItemMeta(value);
            }
            if (in.available() != 0) throw new IllegalArgumentException("trailing ItemStack binary data");
            return stack;
        } catch (EOFException malformed) {
            throw new IllegalArgumentException("truncated ItemStack binary data", malformed);
        } catch (IOException malformed) {
            throw new IllegalArgumentException("malformed ItemStack binary data", malformed);
        }
    }

    private static void writeNullableString(DataOutputStream out, String value) throws IOException {
        out.writeBoolean(value != null);
        if (value != null) out.writeUTF(value);
    }

    private static String readNullableString(DataInputStream in) throws IOException {
        return in.readBoolean() ? in.readUTF() : null;
    }

    public int getMaxStackSize() {
        return type.getMaxStackSize();
    }

    public boolean containsEnchantment(org.bukkit.enchantments.Enchantment enchantment) { return getEnchantmentLevel(enchantment) > 0; }

    public net.kyori.adventure.text.Component effectiveName() { return net.kyori.adventure.text.Component.text(type.getKeyName()); }

    public int getEnchantmentLevel(org.bukkit.enchantments.Enchantment enchantment) {
        return meta == null ? 0 : meta.getEnchantLevel(enchantment);
    }

    public java.util.List<net.kyori.adventure.text.Component> lore() { return getItemMeta().lore(); }

    public void lore(java.util.List<net.kyori.adventure.text.Component> values) { ItemMeta copy=getItemMeta(); copy.lore(values); setItemMeta(copy); }

    public boolean hasItemMeta() {
        return meta != null;
    }

    /** A copy of the meta, or a fresh empty one.
     *
     * A copy because Bukkit's is: a plugin mutates what it gets and then calls
     * setItemMeta, and a plugin that forgets the second call sees no change.
     * That is a trap, it is Bukkit's trap, and behaving differently here would
     * make plugins written against it silently wrong instead.
     */
    public ItemMeta getItemMeta() {
        return meta == null ? emptyMeta() : meta.clone();
    }

    public boolean setItemMeta(ItemMeta value) {
        if (value instanceof org.bukkit.inventory.meta.BookMeta && !isBook()) {
            return false;
        }
        this.meta = value == null ? null : value.clone();
        return true;
    }

    private ItemMeta emptyMeta() {
        if (type == Material.SHULKER_BOX || type == Material.WHITE_SHULKER_BOX || type == Material.ORANGE_SHULKER_BOX || type == Material.MAGENTA_SHULKER_BOX || type == Material.LIGHT_BLUE_SHULKER_BOX || type == Material.YELLOW_SHULKER_BOX || type == Material.LIME_SHULKER_BOX || type == Material.PINK_SHULKER_BOX || type == Material.GRAY_SHULKER_BOX || type == Material.LIGHT_GRAY_SHULKER_BOX || type == Material.CYAN_SHULKER_BOX || type == Material.PURPLE_SHULKER_BOX || type == Material.BLUE_SHULKER_BOX || type == Material.BROWN_SHULKER_BOX || type == Material.GREEN_SHULKER_BOX || type == Material.RED_SHULKER_BOX || type == Material.BLACK_SHULKER_BOX)
            return new org.bukkit.inventory.meta.SimpleBlockStateMeta();
        if (type == Material.POTION || type == Material.SPLASH_POTION || type == Material.LINGERING_POTION || type == Material.TIPPED_ARROW) return new org.bukkit.inventory.meta.SimplePotionMeta();
        if (type == Material.FIREWORK_ROCKET || type == Material.FIREWORK_STAR) return new org.bukkit.inventory.meta.SimpleFireworkMeta();
        if (type == Material.BUNDLE) return new org.bukkit.inventory.meta.SimpleBundleMeta();
        if (type == Material.CROSSBOW) return new org.bukkit.inventory.meta.SimpleCrossbowMeta();
        if (type == Material.SUSPICIOUS_STEW) return new org.bukkit.inventory.meta.SimpleSuspiciousStewMeta();
        if (type.name().endsWith("_BANNER")) return new org.bukkit.inventory.meta.SimpleBannerMeta();
        if (type == Material.ENCHANTED_BOOK) return new org.bukkit.inventory.meta.SimpleEnchantmentStorageMeta();
        return isBook() ? new org.bukkit.inventory.meta.SimpleBookMeta() : new SimpleItemMeta();
    }

    private boolean isBook() {
        return type == Material.WRITABLE_BOOK || type == Material.WRITTEN_BOOK;
    }

    /** Whether two stacks are the same item, ignoring how many. */
    public boolean isSimilar(ItemStack other) {
        return other != null
            && type == other.type
            && durability == other.durability
            && java.util.Objects.equals(meta, other.meta);
    }

    @Override
    public ItemStack clone() {
        ItemStack copy = new ItemStack(type, amount);
        copy.durability = durability;
        copy.meta = meta == null ? null : meta.clone();
        copy.opaqueNbt = opaqueNbt;
        copy.dataComponents.putAll(dataComponents);
        return copy;
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof ItemStack stack
            && amount == stack.amount
            && isSimilar(stack);
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(type, amount, durability, meta);
    }

    @Override
    public String toString() {
        return "ItemStack{" + type + " x " + amount + "}";
    }
}
