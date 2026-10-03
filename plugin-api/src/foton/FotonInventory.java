package foton;

import org.bukkit.Material;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.PlayerInventory;
import org.bukkit.inventory.InventoryHolder;

/** Player inventory snapshots own immutable native item state. Bulk writes stage
 * every candidate before the native inventory is changed. */
public final class FotonInventory implements PlayerInventory {
    /** The transfer owns its lease throughout hydration; the resulting metadata shares it. */
    public static ItemStack decodeTransfer(foton.item.ItemTransfer transfer) {
        if (transfer == null) return null;
        ItemStack result = decode(transfer.projection());
        if (result == null) throw new IllegalArgumentException("invalid native item projection");
        result.attachNativeState(transfer);
        return result;
    }
    /** Vanilla's player inventory: 36 storage slots, 4 armor, 1 offhand. */
    private static final int SIZE = 41;
    private static final int ARMOR = 36;
    private static final int OFFHAND = 40;
    private static final int MAX_OPAQUE_PDC_HEX_LENGTH = 4_194_304;
    private static final int MAX_SLOT_BRIDGE_CHARS = 8_388_608;
    private static final int MAX_SLOT_METADATA_FIELDS = 4_096;

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
        return decodeTransfer(Native.inventorySlot(owner, slot));
    }

    @Override
    public void setItem(int slot, ItemStack item) {
        Native.setInventorySlot(owner, slot, mutation(item));
    }

    @Override
    public java.util.HashMap<Integer, ItemStack> addItem(ItemStack... items) {
        java.util.HashMap<Integer, ItemStack> leftovers = new java.util.HashMap<>();
        if (items == null) return leftovers;
        ItemStack[] contents = getContents();
        for (int index = 0; index < items.length; index++) {
            ItemStack incoming = items[index] == null ? null : items[index].clone();
            if (incoming == null || incoming.getType().isAir() || incoming.getAmount() <= 0) continue;
            for (int slot = 0; slot < getSize() && incoming.getAmount() > 0; slot++) {
                ItemStack current = contents[slot];
                if (current != null && current.isSimilar(incoming)) {
                    int space = current.getMaxStackSize() - current.getAmount();
                    if (space > 0) {
                        int moved = Math.min(space, incoming.getAmount());
                        current.setAmount(current.getAmount() + moved);
                        incoming.setAmount(incoming.getAmount() - moved);
                        contents[slot] = current;
                    }
                }
            }
            for (int slot = 0; slot < getSize() && incoming.getAmount() > 0; slot++) {
                ItemStack current = contents[slot];
                if (current == null || current.getType().isAir() || current.getAmount() <= 0) {
                    int moved = Math.min(incoming.getMaxStackSize(), incoming.getAmount());
                    ItemStack placed = incoming.clone();
                    placed.setAmount(moved);
                    contents[slot] = placed;
                    incoming.setAmount(incoming.getAmount() - moved);
                }
            }
            if (incoming.getAmount() > 0) leftovers.put(index, incoming);
        }
        setContents(contents);
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
        setPlayerContents(owner, false, SIZE, items);
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
        setContents(null);
    }

    @Override
    public void clear(int slot) {
        Native.setInventorySlot(owner, slot, foton.item.ItemMutation.empty());
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
        setPlayerRange(owner, false, 0, ARMOR, contents);
    }

    @Override public void setArmorContents(ItemStack[] contents) {
        setPlayerRange(owner, false, ARMOR, 4, contents);
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
        if ("!foton:item-metadata-limit".equals(text)) {
            throw new IllegalStateException("Native item metadata exceeds the item bridge limit");
        }
        if (text == null || text.isEmpty() || text.length() > MAX_SLOT_BRIDGE_CHARS) {
            return null;
        }
        String[] encoded = text.split("\\u001d", MAX_SLOT_METADATA_FIELDS + 2);
        if (encoded.length > MAX_SLOT_METADATA_FIELDS + 1
                || encoded[encoded.length - 1].indexOf('\u001d') >= 0) return null;
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
            org.bukkit.inventory.meta.ItemMeta damageMeta = result.getItemMeta();
            ((org.bukkit.inventory.meta.Damageable) damageMeta).setDamage(Integer.parseInt(encoded[i].substring(7)));
            result.hydrateMeta(damageMeta);
        }
        for (String field : encoded) if (field.startsWith("nbthex=")) {
            String hex = field.substring(7); java.io.ByteArrayOutputStream raw = new java.io.ByteArrayOutputStream();
            for (int i = 0; i + 1 < hex.length(); i += 2) try { raw.write(Integer.parseInt(hex.substring(i, i + 2), 16)); } catch (NumberFormatException ignored) { raw.reset(); break; }
            if (raw.size() > 0) result.setOpaqueNbt(new String(raw.toByteArray(), java.nio.charset.StandardCharsets.UTF_8));
        }
        if (encoded.length > 1 && result.getItemMeta() instanceof org.bukkit.inventory.meta.ItemMeta meta) {
            if (meta.getPersistentDataContainer() instanceof FotonPersistentDataContainer persistentData) {
                for (String field : encoded) if (field.startsWith("pdcrawhex=")) {
                    if (field.length() > 10 + MAX_OPAQUE_PDC_HEX_LENGTH) continue;
                    String raw = field.substring(10);
                    if (isHex(raw))
                        persistentData.setNativePassthrough(raw);
                }
                for (String field : encoded) if (field.startsWith("pdcidentity=")) {
                    String identity = field.substring(12);
                    if (identity.length() == 64 && isHex(identity))
                        persistentData.setNativePassthroughIdentity(identity);
                }
                for (String field : encoded) persistentData.decodeItemField(field);
            }
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
            for (String field : encoded) if (field.startsWith("namejsonhex=")) {
                try { meta.displayName(ComponentJson.parse(new String(hexDecode(field.substring(12)), java.nio.charset.StandardCharsets.UTF_8))); }
                catch (RuntimeException malformed) { return null; }
            }
            java.util.ArrayList<String> lore = new java.util.ArrayList<>();
            for (String field : encoded) if (field.startsWith("lorehex=")) lore.add(new String(hexDecode(field.substring(8)), java.nio.charset.StandardCharsets.UTF_8));
            if (!lore.isEmpty()) meta.setLore(lore);
            if (meta instanceof org.bukkit.inventory.meta.BookMeta book) {
                java.util.ArrayList<net.kyori.adventure.text.Component> pages = new java.util.ArrayList<>();
                String nativeBook = null;
                boolean nativeBookResolved = false;
                for (String field : encoded) {
                    if (field.startsWith("booktitlehex=")) book.setTitle(new String(hexDecode(field.substring(13)), java.nio.charset.StandardCharsets.UTF_8));
                    else if (field.startsWith("bookauthorhex=")) book.setAuthor(new String(hexDecode(field.substring(14)), java.nio.charset.StandardCharsets.UTF_8));
                    else if (field.startsWith("bookgen=")) {
                        try {
                            int generation = Integer.parseInt(field.substring(8));
                            if (generation >= 0 && generation < org.bukkit.inventory.meta.BookMeta.Generation.values().length)
                                book.setGeneration(org.bukkit.inventory.meta.BookMeta.Generation.values()[generation]);
                        } catch (NumberFormatException malformed) { return null; }
                    } else if (field.startsWith("bookpagehex=")) {
                        String page = new String(hexDecode(field.substring(12)), java.nio.charset.StandardCharsets.UTF_8);
                        try { pages.add(material == Material.WRITTEN_BOOK ? ComponentJson.parse(page) : net.kyori.adventure.text.Component.text(page)); }
                        catch (RuntimeException malformed) { return null; }
                    } else if (field.startsWith("bookrawhex=")) {
                        nativeBook = field.substring(11);
                        if (nativeBook.length() > MAX_SLOT_BRIDGE_CHARS || !isHex(nativeBook)) return null;
                    } else if (field.startsWith("bookresolved=")) {
                        String value = field.substring(13);
                        if (!value.equals("true") && !value.equals("false")) return null;
                        nativeBookResolved = Boolean.parseBoolean(value);
                    }
                }
                if (!pages.isEmpty()) book.pages(pages);
                if (book instanceof org.bukkit.inventory.meta.SimpleBookMeta simple) {
                    simple.setNativeBookPassthrough(nativeBook);
                    simple.setNativeBookResolved(nativeBookResolved);
                }
            }
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
            result.hydrateMeta(meta);
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
            result.hydrateMeta(meta);
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

    private static boolean isHex(String value) {
        if ((value.length() & 1) != 0) return false;
        for (int index = 0; index < value.length(); index++)
            if (Character.digit(value.charAt(index), 16) < 0) return false;
        return true;
    }

    /** Writes what decode reads. An empty stack is an empty string. */
    public static String encode(ItemStack item) {
        if (item == null || item.getType().isAir() || item.getAmount() <= 0) {
            return "";
        }
        String value = "minecraft:" + item.getType().getKeyName() + " " + item.getAmount();
        if (item.getOpaqueNbt() != null && !item.getOpaqueNbt().isEmpty()) value += "\u001dnbthex=" + hexEncode(item.getOpaqueNbt());
        if (item.getItemMeta() instanceof org.bukkit.inventory.meta.Damageable damageable && damageable.getDamage() != 0)
            value += "\u001ddamage=" + damageable.getDamage();
        if ((item.getItemMeta() != null)
                && item.getItemMeta().getPersistentDataContainer() instanceof FotonPersistentDataContainer persistentData) {
            if (persistentData.nativePassthrough() != null && !persistentData.nativePassthrough().isEmpty())
                value += "\u001dpdcrawhex=" + persistentData.nativePassthrough();
            if (persistentData.nativePassthroughIdentity() != null)
                value += "\u001dpdcidentity=" + persistentData.nativePassthroughIdentity();
            value += persistentData.encodeItemFields();
        }
        if ((item.getItemMeta() != null) && item.getItemMeta().isUnbreakable()) value += "\u001dunbreakable";
        if ((item.getItemMeta() != null) && item.getItemMeta().isHideTooltip()) value += "\u001dhidetooltip";
        if ((item.getItemMeta() != null) && item.getItemMeta().hasItemModel())
            value += "\u001ditemmodelhex=" + hexEncode(item.getItemMeta().getItemModel().toString());
        if ((item.getItemMeta() != null) && item.getItemMeta().hasTooltipStyle())
            value += "\u001dtooltipstylehex=" + hexEncode(item.getItemMeta().getTooltipStyle().toString());
        if ((item.getItemMeta() != null) && item.getItemMeta().hasCustomModelData()
                && item.getItemMeta().getCustomModelDataComponent().getFloats().isEmpty())
            value += "\u001dmodel=" + item.getItemMeta().getCustomModelData();
        if ((item.getItemMeta() != null)) {
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
        if ((item.getItemMeta() != null) && item.getItemMeta().hasDisplayName()) {
            String name = item.getItemMeta().getDisplayName();
            StringBuilder hex = new StringBuilder();
            for (byte byteValue : name.getBytes(java.nio.charset.StandardCharsets.UTF_8)) hex.append(String.format("%02x", byteValue & 0xff));
            value += "\u001dnamehex=" + hex;
            value += "\u001dnamejsonhex=" + hexEncode(ComponentJson.json(item.getItemMeta().displayName()));
        }
        if (item.getItemMeta() instanceof org.bukkit.inventory.meta.BookMeta book) {
            if (item.getType() == Material.WRITTEN_BOOK) {
                value += "\u001dbooktitlehex=" + hexEncode(book.hasTitle() ? book.getTitle() : "");
                value += "\u001dbookauthorhex=" + hexEncode(book.hasAuthor() ? book.getAuthor() : "");
                value += "\u001dbookgen=" + (book.getGeneration() == null ? 0 : book.getGeneration().ordinal());
                for (net.kyori.adventure.text.Component page : book.pages())
                    value += "\u001dbookpagehex=" + hexEncode(ComponentJson.json(page));
            } else if (item.getType() == Material.WRITABLE_BOOK) {
                for (String page : book.getPages()) value += "\u001dbookpagehex=" + hexEncode(page);
            }
            if (book instanceof org.bukkit.inventory.meta.SimpleBookMeta simple
                    && simple.nativeBookPassthrough() != null)
                value += "\u001dbookrawhex=" + simple.nativeBookPassthrough();
            if (item.getType() == Material.WRITTEN_BOOK
                    && book instanceof org.bukkit.inventory.meta.SimpleBookMeta simple)
                value += "\u001dbookresolved=" + simple.nativeBookResolved();
        }
        if ((item.getItemMeta() != null)) {
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
        if ((item.getItemMeta() != null) && item.getItemMeta().hasLore()) {
            for (String line : item.getItemMeta().getLore()) {
                StringBuilder hex = new StringBuilder();
                for (byte byteValue : line.getBytes(java.nio.charset.StandardCharsets.UTF_8)) hex.append(String.format("%02x", byteValue & 0xff));
                value += "\u001dlorehex=" + hex;
            }
        }
        if (item.getItemMeta() instanceof org.bukkit.inventory.meta.PotionMeta meta && !meta.getCustomEffects().isEmpty()) {
            StringBuilder effects = new StringBuilder("\u001d");
            for (org.bukkit.potion.PotionEffect effect : meta.getCustomEffects()) {
                if (effect.isAmbient() || !effect.hasParticles() || !effect.hasIcon())
                    throw new UnsupportedOperationException("unrepresented potion effect flags");
                effects.append(effect.getType().getName()).append(',').append(effect.getDuration()).append(',').append(effect.getAmplifier()).append(';');
            }
            value += effects;
        }
        if (value.length() > MAX_SLOT_BRIDGE_CHARS)
            throw new IllegalArgumentException("item metadata exceeds the native bridge limit");
        return value;
    }

    public static foton.item.ItemMutation mutation(ItemStack item) {
        return item == null ? foton.item.ItemMutation.empty() : item.nativeMutation();
    }

    static void setPlayerContents(String owner, boolean ender, int size, ItemStack[] items) {
        setPlayerRange(owner, ender, 0, size, items);
    }

    private static void setPlayerRange(String owner, boolean ender, int start, int size, ItemStack[] items) {
        int[] slots = new int[size];
        foton.item.ItemMutation[] mutations = mutations(size, items);
        for (int slot = 0; slot < size; slot++) {
            slots[slot] = start + slot;
        }
        Native.setPlayerInventorySlots(owner, ender, slots, mutations);
    }

    static foton.item.ItemMutation[] mutations(int size, ItemStack[] items) {
        if (items != null && items.length > size) throw new IllegalArgumentException("inventory contents exceed size");
        foton.item.ItemMutation[] result = new foton.item.ItemMutation[size];
        for (int slot = 0; slot < size; slot++) result[slot] = mutation(items != null && slot < items.length ? items[slot] : null);
        return result;
    }
}
