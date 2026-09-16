/** Paper 26.2 potion ABI and registry-value checks. */
final class PotionEffectCheck {
    private PotionEffectCheck() {}

    static void check() {
        Class<org.bukkit.potion.PotionEffectType> type =
            org.bukkit.potion.PotionEffectType.class;
        Checks.expect(java.lang.reflect.Modifier.isAbstract(type.getModifiers()),
            "PotionEffectType must remain abstract");
        Checks.same(java.util.Arrays.stream(type.getInterfaces()).map(Class::getName).toList(),
            java.util.List.of("org.bukkit.Keyed", "org.bukkit.Translatable",
                "net.kyori.adventure.translation.Translatable",
                "io.papermc.paper.world.flag.FeatureDependant"),
            "PotionEffectType direct Paper interfaces");
        Checks.expect(java.lang.reflect.Modifier.isAbstract(
                org.bukkit.potion.PotionEffectTypeWrapper.class.getModifiers())
                && org.bukkit.potion.PotionEffectTypeWrapper.class.getSuperclass() == type,
            "PotionEffectTypeWrapper Paper ABI");
        Checks.same(org.bukkit.potion.PotionEffectType.values().length, 40,
            "Paper 26.2 potion effect count");
        Checks.same(org.bukkit.potion.PotionEffectType.INSTANT_HEALTH.getId(), 6,
            "instant health registry id");
        Checks.same(org.bukkit.potion.PotionEffectType.RAID_OMEN.getId(), 35,
            "raid omen registry id");
        Checks.same(org.bukkit.potion.PotionEffectType.RAID_OMEN.getCategory(),
            org.bukkit.potion.PotionEffectTypeCategory.NEUTRAL,
            "raid omen category");
        Checks.same(org.bukkit.potion.PotionEffectType.RAID_OMEN.getColor().asRGB(), 0xDE4058,
            "raid omen color");
        Checks.same(org.bukkit.potion.PotionEffectType.SLOWNESS.getName(), "SLOW",
            "legacy slowness name");
        Checks.same(org.bukkit.potion.PotionEffectType.RAID_OMEN.getName(),
            "minecraft:raid_omen", "modern namespaced legacy name");
        Checks.expect(org.bukkit.potion.PotionEffectType.getByName("SLOW") == null
                && org.bukkit.potion.PotionEffectType.getByName("slowness")
                    == org.bukkit.potion.PotionEffectType.SLOWNESS,
            "getByName resolves registry keys rather than legacy display names");
        try {
            org.bukkit.potion.PotionEffectType.getByName(null);
            throw new AssertionError("getByName(null) must reject null");
        } catch (IllegalArgumentException expected) { }
        Checks.expect(org.bukkit.potion.PotionEffectType.getByKey(null) == null,
            "getByKey(null) must return null");

        var deepest = new org.bukkit.potion.PotionEffect(
            org.bukkit.potion.PotionEffectType.LUCK, 600, 4, true, false, true);
        var hidden = new org.bukkit.potion.PotionEffect(
            org.bukkit.potion.PotionEffectType.LUCK, 400, 3, false, true, false, deepest);
        var visible = new org.bukkit.potion.PotionEffect(
            org.bukkit.potion.PotionEffectType.LUCK, 80, 2, true, false, true, hidden);
        Checks.expect(visible.getHiddenPotionEffect() == hidden
                && hidden.getHiddenPotionEffect() == deepest,
            "PotionEffect hidden chain must remain live through the seven-argument constructor");
        java.util.Map<String, Object> serialized = visible.serialize();
        Checks.same(new java.util.ArrayList<>(serialized.keySet()),
            java.util.List.of("effect", "duration", "amplifier", "ambient", "has-particles",
                "has-icon", "hidden_effect"),
            "PotionEffect serialized field order");
        Checks.expect(serialized.get("hidden_effect") == hidden,
            "PotionEffect serialization must retain the hidden object");
        int expectedHash = 1;
        expectedHash = expectedHash * 31 + visible.getType().hashCode();
        expectedHash = expectedHash * 31 + visible.getAmplifier();
        expectedHash = expectedHash * 31 + visible.getDuration();
        expectedHash ^= 0x22222222 >> (visible.isAmbient() ? 1 : -1);
        expectedHash ^= 0x22222222 >> (visible.hasParticles() ? 1 : -1);
        expectedHash ^= 0x22222222 >> (visible.hasIcon() ? 1 : -1);
        expectedHash = expectedHash * 31 + hidden.hashCode();
        Checks.same(visible.hashCode(), expectedHash, "PotionEffect Paper hash code");
        Checks.expect(visible.toString().startsWith("PotionEffect{amplifier=2, duration=80")
                && visible.toString().endsWith("}"),
            "PotionEffect Paper toString shape");
    }
}
