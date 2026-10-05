import org.bukkit.Material;
import org.bukkit.inventory.ItemFlag;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.inventory.meta.SimpleBookMeta;

final class ItemPersistenceCheck {
    private ItemPersistenceCheck() {}

    static void check() {
        ItemStack plain = new ItemStack(Material.STONE, 3);
        ItemMeta meta = plain.getItemMeta();
        meta.setDisplayName("represented name");
        meta.setLore(java.util.List.of("represented lore"));
        meta.setCustomModelData(7);
        meta.setUnbreakable(true);
        plain.setItemMeta(meta);
        Checks.expect(plain.equals(ItemStack.deserialize(plain.serialize())), "represented legacy map roundtrip");
        Checks.expect(plain.equals(ItemStack.deserializeBytes(plain.serializeAsBytes())), "represented legacy bytes roundtrip");
        rejects(meta::serialize, "standalone nonempty metadata must not serialize as emptyMap");

        meta.addItemFlags(ItemFlag.HIDE_ATTRIBUTES);
        plain.setItemMeta(meta);
        rejects(plain::serialize, "map omits flags");
        Checks.expect(plain.equals(ItemStack.deserializeBytes(plain.serializeAsBytes())), "binary represents legacy flags");

        var partialFlags = new org.bukkit.inventory.meta.SimpleItemMeta();
        partialFlags.setHiddenComponents(java.util.List.of("minecraft:bees"));
        ItemStack partial = new ItemStack(Material.STONE);
        partial.setItemMeta(partialFlags);
        rejects(partial::serializeAsBytes, "partial additional-tooltip group is not a representable legacy flag");
        partialFlags.setHiddenComponents(java.util.List.of("fixture:opaque"));
        partial.setItemMeta(partialFlags);
        rejects(partial::serializeAsBytes, "unknown hidden component must not silently disappear");

        ItemStack opaque = new ItemStack(Material.STONE);
        opaque.setOpaqueNbt("{unrepresented:1}");
        rejects(opaque::serialize, "map omits opaque NBT");
        rejects(opaque::serializeAsBytes, "bytes omit opaque NBT");

        SimpleBookMeta book = new SimpleBookMeta();
        book.setPages("unrepresented page");
        rejects(book::serialize, "specialized default serialization cannot return emptyMap");
        ItemStack bookItem = new ItemStack(Material.WRITABLE_BOOK);
        bookItem.setItemMeta(book);
        rejects(bookItem::serializeAsBytes, "legacy bytes omit book pages");

        ItemStack persistent = new ItemStack(Material.STONE);
        ItemMeta persistentMeta = persistent.getItemMeta();
        persistentMeta.getPersistentDataContainer().set(new org.bukkit.NamespacedKey("test", "value"),
            org.bukkit.persistence.PersistentDataType.INTEGER, 4);
        persistent.setItemMeta(persistentMeta);
        rejects(persistent::serialize, "map omits PDC");
        rejects(persistent::serializeAsBytes, "bytes omit PDC");
    }

    private static void rejects(Runnable operation, String message) {
        try { operation.run(); }
        catch (UnsupportedOperationException expected) { return; }
        throw new AssertionError(message);
    }
}
