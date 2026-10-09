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
 * over a description rather than a live reference into a chest -- writing an
 * inventory takes copies. Reading a player's inventory slot gives a mirror
 * that writes edits back (see {@code foton.FotonItemMirror}); every other
 * read, and {@code clone()}, gives a detached copy.
 */
public class ItemStack implements Cloneable {
    private Material type;
    private int amount;
    private ItemMeta meta;
    /** Opaque NBT retained by Foton for plugin round-tripping. */
    private String opaqueNbt;

    public ItemStack(Material type) {
        this(type, 1);
    }

    public ItemStack(Material type, int amount, short durability) { this(type, amount); initializeDurability(durability); }

    /** Legacy constructor retained for ViaVersion and other pre-flattening integrations. */
    @Deprecated
    public ItemStack(Material type, int amount, short durability, Byte data) {
        this(type, amount, durability);
        if (data != null) initializeDurability(data);
    }

    public ItemStack(Material type, int amount) {
        if (amount <= 0) throw new IllegalArgumentException("amount must be greater than 0");
        this.type = type == null ? Material.AIR : type;
        this.amount = amount;
    }

    public Material getType() {
        return amount <= 0 ? Material.AIR : type;
    }

    /** Changing the type to air empties the stack, which is what Bukkit does
     * and what a plugin clearing a slot relies on. */
    public void setType(Material type) {
        Material target = type == null ? Material.AIR : type;
        if (target.isAir()) {
            this.type = target;
            this.amount = 0;
            this.meta = null;
            this.opaqueNbt = null;
            return;
        }
        if (isEmpty()) {
            if (this.type.isAir()) amount = 1;
            this.type = target;
            meta = null;
            opaqueNbt = null;
            return;
        }
        if (foton.PluginHost.itemBridgeBound()) {
            ItemStack rebased = foton.FotonInventory.decodeTransfer(foton.Native.rebaseItem(nativeMutation(),
                "minecraft:" + target.getKeyName(), false));
            if (rebased == null) throw new IllegalStateException("nonempty item rebase returned empty state");
            adopt(rebased);
            return;
        }
        this.type = target;
    }

    /** Takes on another stack's content, keeping this object: a rebase or a mirror refresh. */
    public void adopt(ItemStack other) {
        this.type = other.type;
        this.amount = other.amount;
        this.meta = other.meta;
        this.opaqueNbt = other.opaqueNbt;
    }

    public int getAmount() {
        return amount;
    }

    public void setAmount(int amount) {
        this.amount = amount;
    }

    @Deprecated public org.bukkit.material.MaterialData getData() { return new org.bukkit.material.MaterialData(type, (byte) getDurability()); }

    public short getDurability() {
        if (foton.PluginHost.itemBridgeBound()) return (short) foton.Native.itemDurability(nativeMutation());
        return (short) damageValue();
    }

    private int damageValue() { return meta instanceof Damageable damageable ? damageable.getDamage() : 0; }

    public String getOpaqueNbt() { return opaqueNbt; }
    public void setOpaqueNbt(String value) {
        if (!java.util.Objects.equals(opaqueNbt, value)
                && meta instanceof SimpleItemMeta simple && simple.nativeState().hasNativeBase())
            throw new UnsupportedOperationException("carrier opaque NBT mutation");
        opaqueNbt = value;
    }

    private static String componentKey(io.papermc.paper.datacomponent.DataComponentType<?> type) {
        if (type == null) throw new IllegalArgumentException("component type");
        if (type == io.papermc.paper.datacomponent.DataComponentTypes.DAMAGE) return "damage";
        if (type == io.papermc.paper.datacomponent.DataComponentTypes.CUSTOM_MODEL_DATA) return "custom_model_data";
        if (type == io.papermc.paper.datacomponent.DataComponentTypes.BANNER_PATTERNS) return "banner_patterns";
        if (type == io.papermc.paper.datacomponent.DataComponentTypes.BASE_COLOR) return "base_color";
        if (type == io.papermc.paper.datacomponent.DataComponentTypes.TRIM) return "trim";
        throw new UnsupportedOperationException("public component key");
    }

    private static void requireNativeComponents() {
        if (!foton.PluginHost.itemBridgeBound())
            throw new IllegalStateException("canonical item components require native registry binding");
    }

    public <T> void setData(io.papermc.paper.datacomponent.DataComponentType.Valued<T> type, T value) {
        componentKey(type);
        if (setMetaComponent(type, value)) return;
        requireNativeComponents();
        if (value == null) throw new IllegalArgumentException("component value");
        if (isEmpty()) return;
        ItemMeta copy = getItemMeta();
        if (type == io.papermc.paper.datacomponent.DataComponentTypes.DAMAGE) {
            if (!(value instanceof Integer damage)) throw new IllegalArgumentException("damage must be an integer");
            ((Damageable) copy).setDamage(damage);
        } else {
            if (!(value instanceof io.papermc.paper.datacomponent.item.CustomModelData model))
                throw new IllegalArgumentException("custom model data value");
            org.bukkit.inventory.meta.components.CustomModelDataComponent target = copy.getCustomModelDataComponent();
            target.setFloats(model.floats());
            target.setFlags(model.flags());
            target.setStrings(model.strings());
            java.util.ArrayList<org.bukkit.Color> colors = new java.util.ArrayList<>();
            for (Integer color : model.colors()) colors.add(org.bukkit.Color.fromRGB(color));
            target.setColors(colors);
            copy.setCustomModelDataComponent(target);
        }
        setItemMeta(copy);
    }

    @SuppressWarnings("unchecked")
    public <T> T getData(io.papermc.paper.datacomponent.DataComponentType<T> type) {
        componentKey(type);
        if (isMetaComponent(type)) {
            if (foton.PluginHost.itemBridgeBound() && !hasData(type)) return null;
            return (T) metaComponent(type);
        }
        requireNativeComponents();
        return (T) (type == io.papermc.paper.datacomponent.DataComponentTypes.DAMAGE
            ? foton.Native.itemDamage(nativeMutation()) : foton.Native.itemCustomModelData(nativeMutation()));
    }

    private static boolean isMetaComponent(io.papermc.paper.datacomponent.DataComponentType<?> type) {
        return type == io.papermc.paper.datacomponent.DataComponentTypes.BANNER_PATTERNS
            || type == io.papermc.paper.datacomponent.DataComponentTypes.BASE_COLOR
            || type == io.papermc.paper.datacomponent.DataComponentTypes.TRIM;
    }

    /** A component that lives in the meta, as a Paper component value; null when absent. */
    private Object metaComponent(io.papermc.paper.datacomponent.DataComponentType<?> type) {
        if (!(meta instanceof SimpleItemMeta simple)) return null;
        if (type == io.papermc.paper.datacomponent.DataComponentTypes.BANNER_PATTERNS) {
            List<org.bukkit.block.banner.Pattern> patterns = simple.getBannerPatternsComponent();
            return patterns == null ? null : io.papermc.paper.datacomponent.item.BannerPatternLayers.bannerPatternLayers(patterns);
        }
        if (type == io.papermc.paper.datacomponent.DataComponentTypes.BASE_COLOR) return simple.getBaseColorComponent();
        org.bukkit.inventory.meta.trim.ArmorTrim trim = simple.getTrimComponent();
        return trim == null ? null : io.papermc.paper.datacomponent.item.ItemArmorTrim.itemArmorTrim(trim).build();
    }

    /** Writes a meta-held component (null removes it); false when the type is not one. */
    private boolean setMetaComponent(io.papermc.paper.datacomponent.DataComponentType<?> type, Object value) {
        if (!isMetaComponent(type)) return false;
        if (!(getItemMeta() instanceof SimpleItemMeta copy)) return true;
        if (type == io.papermc.paper.datacomponent.DataComponentTypes.BANNER_PATTERNS) {
            copy.setBannerPatternsComponent(value == null ? null
                : ((io.papermc.paper.datacomponent.item.BannerPatternLayers) value).patterns());
        } else if (type == io.papermc.paper.datacomponent.DataComponentTypes.BASE_COLOR) {
            copy.setBaseColorComponent((org.bukkit.DyeColor) value);
        } else {
            copy.setTrimComponent(value == null ? null
                : ((io.papermc.paper.datacomponent.item.ItemArmorTrim) value).armorTrim());
        }
        setItemMeta(copy);
        return true;
    }

    /** The stack's persistent data, read-only: a copy, so writing to it changes nothing. */
    public io.papermc.paper.persistence.PersistentDataContainerView getPersistentDataContainer() {
        if (!(meta instanceof SimpleItemMeta simple)) return new foton.FotonPersistentDataContainer();
        return ((foton.FotonPersistentDataContainer) simple.getPersistentDataContainer()).copy();
    }
    public <T> T getData(io.papermc.paper.datacomponent.DataComponentType.Valued<T> type) {
        return getData((io.papermc.paper.datacomponent.DataComponentType<T>) type);
    }

    public boolean hasData(io.papermc.paper.datacomponent.DataComponentType<?> type) {
        String key = componentKey(type);
        if (!foton.PluginHost.itemBridgeBound() && isMetaComponent(type)) return metaComponent(type) != null;
        requireNativeComponents();
        int state = foton.Native.itemComponentState(nativeMutation(), key);
        return state == 1 || state == 2;
    }

    public boolean isDataOverridden(io.papermc.paper.datacomponent.DataComponentType<?> type) {
        String key = componentKey(type);
        requireNativeComponents();
        return foton.Native.itemComponentState(nativeMutation(), key) >= 2;
    }

    /** Removes the effective component, including a value supplied by the prototype. */
    public void unsetData(io.papermc.paper.datacomponent.DataComponentType<?> type) { editComponent(type, false); }

    /** Clears the patch entry so that the current item's prototype becomes visible. */
    public void resetData(io.papermc.paper.datacomponent.DataComponentType<?> type) { editComponent(type, true); }

    private void editComponent(io.papermc.paper.datacomponent.DataComponentType<?> type, boolean reset) {
        String key = componentKey(type);
        if (!foton.PluginHost.itemBridgeBound() && isMetaComponent(type)) {
            setMetaComponent(type, null);
            return;
        }
        requireNativeComponents();
        if (isEmpty()) return;
        ItemStack changed = foton.FotonInventory.decodeTransfer(foton.Native.editItemComponent(nativeMutation(), key, reset));
        if (changed == null) throw new IllegalStateException("component edit returned empty state");
        meta = changed.meta;
        opaqueNbt = changed.opaqueNbt;
    }

    /** Legacy numeric item id; modern materials intentionally do not expose one. */
    @Deprecated
    public int getTypeId() { return type.getId(); }

    public void setDurability(short durability) {
        if (isEmpty()) return;
        if (foton.PluginHost.itemBridgeBound()) {
            ItemStack changed = foton.FotonInventory.decodeTransfer(foton.Native.setItemDurability(nativeMutation(), durability));
            if (changed == null) throw new IllegalStateException("durability edit returned empty state");
            meta = changed.meta;
            opaqueNbt = changed.opaqueNbt;
            return;
        }
        ItemMeta copy = getItemMeta();
        ((Damageable) copy).setDamage(Math.max(0, Math.min(Short.toUnsignedInt(type.getMaxDurability()), durability)));
        setItemMeta(copy);
    }

    private void initializeDurability(short durability) {
        ItemMeta initial = emptyMeta();
        ((Damageable) initial).setDamage(Math.max(0, Math.min(Short.toUnsignedInt(type.getMaxDurability()), durability)));
        meta = initial;
    }

    public void addEnchantment(org.bukkit.enchantments.Enchantment enchantment, int level) { if (enchantment == null || level <= 0 || level > enchantment.getMaxLevel()) throw new IllegalArgumentException("Invalid enchantment level"); ItemMeta copy=getItemMeta(); if(copy.addEnchant(enchantment, level, false)) setItemMeta(copy); }

    public void addUnsafeEnchantment(org.bukkit.enchantments.Enchantment enchantment, int level) {
        ItemMeta copy = getItemMeta(); boolean changed = copy.addEnchant(enchantment, level, true); if (changed) setItemMeta(copy);
    }

    public int removeEnchantment(org.bukkit.enchantments.Enchantment enchantment) { ItemMeta copy = getItemMeta(); int old = copy.getEnchantLevel(enchantment); if (copy.removeEnchant(enchantment)) setItemMeta(copy); return old; }

    public java.util.Map<org.bukkit.enchantments.Enchantment, Integer> getEnchantments() { return getItemMeta().getEnchants(); }

    public java.util.Map<String, Object> serialize() { requireLegacyPersistence(false); java.util.Map<String,Object> values=new java.util.LinkedHashMap<>(); values.put("type", type.getKeyName()); values.put("amount", amount); if (getDurability() != 0) values.put("durability", getDurability()); if (meta != null) { java.util.Map<String,Object> m=new java.util.LinkedHashMap<>(); if(meta.hasDisplayName())m.put("display-name",meta.getDisplayName()); if(meta.hasLore())m.put("lore",meta.getLore()); if(meta.hasCustomModelData())m.put("custom-model-data",meta.getCustomModelData()); if(meta.isUnbreakable())m.put("Unbreakable",true); values.put("meta",m); } return values; }

    private void requireLegacyPersistence(boolean binary) {
        if (amount < 0 || !binary && damageValue() > Short.toUnsignedInt(type.getMaxDurability()))
            throw new UnsupportedOperationException("unrepresented legacy item fields");
        if (opaqueNbt != null)
            throw new UnsupportedOperationException("opaque item state persistence");
        if (meta instanceof SimpleItemMeta simple) simple.requireLegacyPersistence(binary);
        else if (meta != null) throw new UnsupportedOperationException("foreign metadata persistence");
    }

    public static ItemStack deserialize(java.util.Map<String, Object> values) {
        if (values == null) throw new IllegalArgumentException("values");
        Object raw = values.get("type"); Material material = raw instanceof Material m ? m : Material.matchMaterial(String.valueOf(raw));
        if (material == null) throw new IllegalArgumentException("Unknown material: " + raw);
        int count = values.get("amount") instanceof Number n ? n.intValue() : 1;
        short damage = values.get("durability") instanceof Number n ? n.shortValue() : 0;
        ItemStack stack = new ItemStack(material, Math.max(1, count)); stack.setDurability(damage); Object rawMeta=values.get("meta"); if(rawMeta instanceof java.util.Map<?,?> m){ ItemMeta meta=stack.getItemMeta(); Object n=m.get("display-name"); if(n instanceof String v)meta.setDisplayName(v); Object l=m.get("lore"); if(l instanceof java.util.List<?> v){ java.util.ArrayList<String> lines=new java.util.ArrayList<>(); for(Object x:v)if(x instanceof String z)lines.add(z); meta.setLore(lines); } Object cmd=m.get("custom-model-data"); if(cmd instanceof Number v)meta.setCustomModelData(v.intValue()); if(Boolean.TRUE.equals(m.get("Unbreakable")))meta.setUnbreakable(true); stack.setItemMeta(meta); } stack.setAmount(count); return stack;
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
            requireLegacyPersistence(true);
        } catch (UnsupportedOperationException unrepresented) {
            // Whatever the legacy layout has no field for goes through the server's own item NBT.
            if (!foton.PluginHost.itemBridgeBound()) throw unrepresented;
            return foton.Native.serializeItem(nativeMutation());
        }
        try {
            ByteArrayOutputStream bytes = new ByteArrayOutputStream();
            DataOutputStream out = new DataOutputStream(bytes);
            out.writeInt(0x46544F4E); // "FTON"
            out.writeByte(1);
            out.writeUTF(type.getKeyName());
            out.writeInt(amount);
            out.writeShort(getDurability());

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
        // A root compound tag begins with 0x0A; the legacy layout begins with "FTON".
        if (bytes.length > 0 && bytes[0] == 0x0A && foton.PluginHost.itemBridgeBound())
            return foton.FotonInventory.decodeTransfer(foton.Native.deserializeItem(bytes));
        try {
            DataInputStream in = new DataInputStream(new ByteArrayInputStream(bytes));
            if (in.readInt() != 0x46544F4E) throw new IllegalArgumentException("invalid ItemStack binary header");
            if (in.readUnsignedByte() != 1) throw new IllegalArgumentException("unsupported ItemStack binary version");
            Material material = Material.matchMaterial(in.readUTF());
            if (material == null) throw new IllegalArgumentException("unknown material in ItemStack binary data");
            int count = in.readInt();
            if (count < 0) throw new IllegalArgumentException("negative legacy item count");
            ItemStack stack = new ItemStack(material, Math.max(1, count));
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
            stack.setAmount(count);
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

    /** The item's max_stack_size component when it has one, else its type's. */
    public int getMaxStackSize() {
        return meta != null && meta.hasMaxStackSize() ? meta.getMaxStackSize() : type.getMaxStackSize();
    }

    public boolean containsEnchantment(org.bukkit.enchantments.Enchantment enchantment) { return getEnchantmentLevel(enchantment) > 0; }

    public net.kyori.adventure.text.Component effectiveName() { return net.kyori.adventure.text.Component.text(type.getKeyName()); }

    public int getEnchantmentLevel(org.bukkit.enchantments.Enchantment enchantment) {
        return meta == null ? 0 : meta.getEnchantLevel(enchantment);
    }

    public java.util.List<net.kyori.adventure.text.Component> lore() { return getItemMeta().lore(); }

    public void lore(java.util.List<net.kyori.adventure.text.Component> values) { ItemMeta copy=getItemMeta(); copy.lore(values); setItemMeta(copy); }

    public boolean hasItemMeta() {
        if (isEmpty()) return false;
        if (foton.PluginHost.itemBridgeBound()) return foton.Native.itemHasMeta(nativeMutation());
        return meta != null;
    }

    /** Whether this stack holds nothing.
     *
     * <p>Air or a non-positive count. Plugins use it instead of the
     * {@code == null || getType() == Material.AIR} dance, and getting it wrong
     * the other way -- reporting a real stack as empty -- silently deletes
     * items in inventory code. */
    public boolean isEmpty() {
        return type == null || type == Material.AIR || getAmount() <= 0;
    }

    /** Edits this stack's meta in place.
     *
     * <p>The reason Paper added it: {@link #getItemMeta()} hands back a copy,
     * so the get-modify-set dance is three statements and forgetting the third
     * is the single most common Bukkit mistake. Returns whether the meta was
     * applied -- false when the stack cannot carry meta at all. */
    public boolean editMeta(java.util.function.Consumer<? super ItemMeta> edit) {
        if (edit == null || isEmpty()) return false;
        ItemMeta working = getItemMeta();
        if (working == null) return false;
        edit.accept(working);
        return setItemMeta(working);
    }

    /** The stack's display name as a component, or null when it has none. */
    public net.kyori.adventure.text.Component displayName() {
        ItemMeta current = getItemMeta();
        return current == null ? null : current.displayName();
    }

    /** Returns detached metadata; edits take effect only after setItemMeta. */
    public ItemMeta getItemMeta() {
        if (isEmpty()) return new SimpleItemMeta();
        return meta == null ? emptyMeta() : meta.clone();
    }

    /** Internal hydration hook; native state is not public serialized metadata. */
    public void attachNativeState(foton.item.ItemTransfer transfer) {
        if (meta == null) meta = emptyMeta();
        if (!(meta instanceof SimpleItemMeta simple))
            throw new IllegalStateException("unsupported item metadata implementation");
        simple.attachNativeState(transfer);
    }

    public foton.item.ItemMutation nativeMutation() {
        if (isEmpty()) return foton.item.ItemMutation.empty();
        if (meta != null && !(meta instanceof SimpleItemMeta))
            throw new UnsupportedOperationException("foreign metadata implementation");
        SimpleItemMeta simple = meta instanceof SimpleItemMeta value ? value : new SimpleItemMeta();
        return simple.nativeState().mutation(foton.FotonInventory.encode(this));
    }

    public boolean setItemMeta(ItemMeta value) {
        if (value != null && !(value instanceof SimpleItemMeta))
            throw new IllegalArgumentException("foreign metadata implementation");
        if (foton.PluginHost.itemBridgeBound()) {
            if (isEmpty()) return false;
            if (value == null) {
                meta = null;
                return true;
            }
            SimpleItemMeta donor = (SimpleItemMeta) value;
            Material source = donor.nativeMaterial() == null ? type : donor.nativeMaterial();
            if (donor.nativeMaterial() == null && donor.nativeFamily().equals("BOOK_WRITABLE")) source = Material.WRITABLE_BOOK;
            ItemStack candidate = new ItemStack(source, amount);
            candidate.meta = donor.clone();
            foton.item.ItemMutation mutation = candidate.nativeMutation();
            if (!foton.Native.itemHasMeta(mutation)) {
                meta = null;
                return true;
            }
            String targetFamily = foton.Native.itemMetaKind("minecraft:" + type.getKeyName());
            if (!foton.item.MetaFamily.applicable(donor.nativeFamily(), targetFamily)) return false;
            if (donor.nativeFamily().equals("BLOCK_STATE") && source != type)
                throw new UnsupportedOperationException("cross-material block-state metadata");
            ItemStack converted = foton.FotonInventory.decodeTransfer(foton.Native.convertItemMeta(
                mutation, nativeMutation(), "minecraft:" + type.getKeyName(),
                donor.nativeFamily().equals("LEATHER_ARMOR") && targetFamily.equals("COLORABLE_ARMOR")));
            if (converted == null) throw new IllegalStateException("nonempty donor conversion returned empty state");
            meta = converted.meta;
            opaqueNbt = converted.opaqueNbt;
            return true;
        }
        if (value instanceof org.bukkit.inventory.meta.BookMeta && !isBook()) {
            return false;
        }
        this.meta = value == null ? null : value.clone();
        return true;
    }

    /** Internal projection hydration; canonical validation is performed by the owning transfer. */
    public void hydrateMeta(ItemMeta value) { meta = value == null ? null : value.clone(); }

    private ItemMeta emptyMeta() {
        if (foton.PluginHost.itemBridgeBound()) return foton.item.MetaFamily.empty(type);
        if (type == Material.SHULKER_BOX || type == Material.WHITE_SHULKER_BOX || type == Material.ORANGE_SHULKER_BOX || type == Material.MAGENTA_SHULKER_BOX || type == Material.LIGHT_BLUE_SHULKER_BOX || type == Material.YELLOW_SHULKER_BOX || type == Material.LIME_SHULKER_BOX || type == Material.PINK_SHULKER_BOX || type == Material.GRAY_SHULKER_BOX || type == Material.LIGHT_GRAY_SHULKER_BOX || type == Material.CYAN_SHULKER_BOX || type == Material.PURPLE_SHULKER_BOX || type == Material.BLUE_SHULKER_BOX || type == Material.BROWN_SHULKER_BOX || type == Material.GREEN_SHULKER_BOX || type == Material.RED_SHULKER_BOX || type == Material.BLACK_SHULKER_BOX)
            return new org.bukkit.inventory.meta.SimpleBlockStateMeta();
        if (type == Material.POTION || type == Material.SPLASH_POTION || type == Material.LINGERING_POTION || type == Material.TIPPED_ARROW) return new org.bukkit.inventory.meta.SimplePotionMeta();
        if (type == Material.FIREWORK_ROCKET || type == Material.FIREWORK_STAR) return new org.bukkit.inventory.meta.SimpleFireworkMeta();
        if (type == Material.BUNDLE) return new org.bukkit.inventory.meta.SimpleBundleMeta();
        if (type == Material.CROSSBOW) return new org.bukkit.inventory.meta.SimpleCrossbowMeta();
        if (type == Material.SUSPICIOUS_STEW) return new org.bukkit.inventory.meta.SimpleSuspiciousStewMeta();
        if (type.name().endsWith("_BANNER")) return new org.bukkit.inventory.meta.SimpleBannerMeta();
        if (type == Material.GOAT_HORN) return new org.bukkit.inventory.meta.SimpleMusicInstrumentMeta();
        if (java.util.Arrays.asList(foton.FotonRegistryData.TRIMMABLE_ARMOR).contains(type.getKeyName())) {
            return type.name().startsWith("LEATHER_")
                ? new org.bukkit.inventory.meta.SimpleLeatherArmorMeta()
                : new org.bukkit.inventory.meta.SimpleArmorMeta();
        }
        if (type == Material.ENCHANTED_BOOK) return new org.bukkit.inventory.meta.SimpleEnchantmentStorageMeta();
        return isBook() ? new org.bukkit.inventory.meta.SimpleBookMeta() : new SimpleItemMeta();
    }

    private boolean isBook() {
        return type == Material.WRITABLE_BOOK || type == Material.WRITTEN_BOOK;
    }

    /** Whether two stacks are the same item, ignoring how many. */
    public boolean isSimilar(ItemStack other) {
        if (other == null) return false;
        if (foton.PluginHost.itemBridgeBound()) return foton.Native.itemsSimilar(nativeMutation(), other.nativeMutation());
        return other != null
            && type == other.type
            && java.util.Objects.equals(meta, other.meta);
    }

    @Override
    public ItemStack clone() {
        ItemStack copy = new ItemStack(type, 1);
        copy.amount = amount;
        copy.meta = meta == null ? null : meta.clone();
        copy.opaqueNbt = opaqueNbt;
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
        // Canonical comparison includes unprojected native state; a coarse hash
        // avoids inventing a persistent component hash or hashing only projections.
        return java.util.Objects.hash(isEmpty() ? Material.AIR : type, amount);
    }

    @Override
    public String toString() {
        return "ItemStack{" + type + " x " + amount + "}";
    }
}
