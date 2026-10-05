import java.util.List;
import java.util.Map;
import org.bukkit.NamespacedKey;
import org.bukkit.inventory.ItemFlag;
import org.bukkit.inventory.meta.SimpleItemMeta;
import org.bukkit.persistence.PersistentDataType;

/** Current-parent setters must notify the canonical journal without aliasing donors. */
public final class MetadataJournalCheck {
    private MetadataJournalCheck() {}
    public static void main(String[] args) throws Exception { check(); }
    public static void check() throws Exception {
        SimpleItemMeta meta = new SimpleItemMeta();
        meta.setEnchantmentGlintOverride(true);
        meta.setMaxStackSize(16);
        meta.setUseCooldown(new org.bukkit.inventory.meta.components.SimpleUseCooldownComponent(2.5f, null));
        meta.setHiddenComponents(List.of("example:opaque"));
        meta.addItemFlags(ItemFlag.HIDE_ATTRIBUTES);
        meta.setHideTooltip(true);
        meta.lore(List.of(net.kyori.adventure.text.Component.text("rich")));
        Map<String, String> edits = edits(meta);
        same(edits.get("enchantment_glint_override"), "SET");
        same(edits.get("max_stack_size"), "SET");
        same(edits.get("use_cooldown"), "SET");
        same(edits.get("tooltip_display"), "SET");
        same(edits.get("lore"), "SET");
        if (edits.keySet().stream().anyMatch(key -> key.startsWith("unsupported:")))
            throw new AssertionError("supported parent edit rejected: " + edits);
        meta.removeItemFlags(ItemFlag.HIDE_ATTRIBUTES);
        same(meta.getHiddenComponents(), List.of("example:opaque"));
        meta.setEnchantmentGlintOverride(null);
        meta.setMaxStackSize(null);
        meta.setUseCooldown(null);
        for (String key : List.of("enchantment_glint_override", "max_stack_size", "use_cooldown"))
            same(edits(meta).get(key), "REMOVE");
        int[] original = {1, 2};
        meta.setCustomData(Map.of("nested", Map.of("array", original)));
        original[0] = 9;
        SimpleItemMeta clone = meta.clone();
        var returned = clone.getCustomData();
        ((int[]) ((Map<?, ?>) returned.get("nested")).get("array"))[0] = 8;
        same(((int[]) ((Map<?, ?>) meta.getCustomData().get("nested")).get("array"))[0], 1);
        same(((int[]) ((Map<?, ?>) clone.getCustomData().get("nested")).get("array"))[0], 1);
        clone.getPersistentDataContainer().set(NamespacedKey.fromString("fixture:key"), PersistentDataType.INTEGER, 42);
        same(edits(clone).get("custom_data"), "SET");
        if (meta.getPersistentDataContainer().has(NamespacedKey.fromString("fixture:key")))
            throw new AssertionError("PDC donor alias");
        SimpleItemMeta opaque = new SimpleItemMeta();
        opaque.setCustomData(Map.of("PublicBukkitValues", Map.of(
            ":invalid", "opaque", "fixture:edit", 7,
            "fixture:nested", Map.of(":nested-invalid", "retained", "fixture:value", 3L))));
        opaque.getPersistentDataContainer().remove(NamespacedKey.fromString("fixture:edit"));
        opaque.getPersistentDataContainer().set(NamespacedKey.fromString("fixture:new"), PersistentDataType.LONG, 42L);
        var raw = (Map<?, ?>) opaque.getCustomData().get("PublicBukkitValues");
        same(raw.get(":invalid"), "opaque");
        same(raw.get("fixture:new"), 42L);
        if (raw.containsKey("fixture:edit")) throw new AssertionError("removed PDC key resurrected");
        same(((Map<?, ?>) raw.get("fixture:nested")).get(":nested-invalid"), "retained");
        same(((Map<?, ?>) raw.get("fixture:nested")).get("fixture:value"), 3L);
        if (opaque.getPersistentDataContainer().getKeys().stream().anyMatch(key -> key.toString().contains("invalid")))
            throw new AssertionError("opaque keys exposed as valid PDC keys");
        try { meta.requireLegacyPersistence(true); throw new AssertionError("lossy persistence accepted"); }
        catch (UnsupportedOperationException expected) { }
    }
    private static Map<String, String> edits(SimpleItemMeta meta) throws Exception {
        // Reflection keeps this check independent of the native lease transport.
        Object state = meta.getClass().getMethod("nativeState").invoke(meta);
        Object mutation = state.getClass().getMethod("mutation", String.class).invoke(state, "minecraft:stone 1");
        var keys = mutation.getClass().getDeclaredField("keys");
        var operations = mutation.getClass().getDeclaredField("operations");
        keys.setAccessible(true);
        operations.setAccessible(true);
        String[] names = (String[]) keys.get(mutation);
        String[] values = (String[]) operations.get(mutation);
        Map<String, String> result = new java.util.LinkedHashMap<>();
        for (int index = 0; index < names.length; index++)
            if (result.put(names[index], values[index]) != null) throw new AssertionError("duplicate journal key");
        return result;
    }
    private static void same(Object actual, Object expected) {
        if (!java.util.Objects.equals(actual, expected)) throw new AssertionError("expected " + expected + ", got " + actual);
    }
}
