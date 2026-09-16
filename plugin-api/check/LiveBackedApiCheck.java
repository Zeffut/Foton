/** Checks Task 6 APIs whose values delegate to live server foundations. */
public final class LiveBackedApiCheck {
    private LiveBackedApiCheck() { }

    static void check() {
        Checks.expect(org.bukkit.OfflinePlayer.class.isAssignableFrom(
            org.bukkit.entity.Player.class), "Player should extend OfflinePlayer");
        Checks.expect(org.bukkit.entity.AnimalTamer.class.isAssignableFrom(
            org.bukkit.OfflinePlayer.class), "OfflinePlayer should extend AnimalTamer");
        Checks.expect(org.bukkit.entity.AnimalTamer.class.getInterfaces().length == 0,
            "AnimalTamer should be independent");
        try {
            org.bukkit.entity.AnimalTamer.class.getDeclaredMethod("getName");
            org.bukkit.entity.AnimalTamer.class.getDeclaredMethod("getUniqueId");
        } catch (ReflectiveOperationException exception) {
            throw new AssertionError("AnimalTamer should declare its identity methods", exception);
        }

        assertRule(org.bukkit.GameRule.ANNOUNCE_ADVANCEMENTS, "show_advancement_messages");
        assertRule(org.bukkit.GameRule.DO_INSOMNIA, "spawn_phantoms");
        assertRule(org.bukkit.GameRule.DO_PATROL_SPAWNING, "spawn_patrols");
        assertRule(org.bukkit.GameRule.DO_TRADER_SPAWNING, "spawn_wandering_traders");

        Checks.same(org.bukkit.Tag.ITEMS_TRIMMABLE_ARMOR.getKey().toString(),
            "minecraft:trimmable_armor", "trimmable armor tag key");
    }

    private static void assertRule(org.bukkit.GameRule<Boolean> rule, String key) {
        Checks.same(rule.getName(), key, "game rule key");
        Checks.expect(org.bukkit.GameRule.getByName(key) == rule,
            "getByName should return the canonical game rule");
        long occurrences = java.util.Arrays.stream(org.bukkit.GameRule.values())
            .filter(candidate -> candidate == rule).count();
        Checks.same(occurrences, 1L, "game rule should occur exactly once in values");
    }
}
