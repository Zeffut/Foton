/** Drives the real Foton dispatcher with a listener compiled only against Paper. */
public final class EnchantmentBridgeCheck {
    public static void main(String[] args) {
        if (foton.PluginHost.loadAll(args[0]) != 1) throw new AssertionError("fixture enable");
        String uuid = "00000000-0000-0000-0000-000000000123";
        String state = "42|123456789;2,minecraft:unbreaking,1;4,minecraft:sharpness,1;0,,-1";
        String cancelled = foton.EnchantmentEventBridge.prepare(uuid, "minecraft:overworld", 8, 64, 8,
            "minecraft:diamond_sword 1", "minecraft:lapis_lazuli 2", 7, state, false);
        if (!cancelled.startsWith("1\u001f")) throw new AssertionError("listener cancellation lost");
        String changed = foton.EnchantmentEventBridge.prepare(uuid, "minecraft:overworld", 8, 64, 8,
            "minecraft:stone 1", "minecraft:lapis_lazuli 2", 7, state, true);
        String expected = "0\u001f-2147483641;9,minecraft:unbreaking,3;0,,-1;0,,-1"
            + "\u001fminecraft:diamond_sword 1\u001fminecraft:lapis_lazuli 3";
        if (!expected.equals(changed)) throw new AssertionError("writeback differs: " + changed);
        if (PaperEnchantFixture.calls != 2) throw new AssertionError("actual listeners not called");
        if (PaperEnchantFixture.completed != 2) throw new AssertionError("listener assertion failed");
        foton.PluginHost.disableAll();
        System.out.println("Paper-compiled prepare enchant: interface ABI, cancellation, offers, seed and inventory passed");
    }
}
