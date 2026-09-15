/** Verifies that legacy potion lookups return the canonical static instances. */
public final class FotonPotionLookupRunner {
    private FotonPotionLookupRunner() {}

    public static void main(String[] args) {
        assertCanonical("haste", org.bukkit.potion.PotionEffectType.HASTE);
        assertCanonical("minecraft:haste", org.bukkit.potion.PotionEffectType.HASTE);
        assertCanonical("jump_boost", org.bukkit.potion.PotionEffectType.JUMP_BOOST);
        assertCanonical("resistance", org.bukkit.potion.PotionEffectType.RESISTANCE);
        assertCanonical("strength", org.bukkit.potion.PotionEffectType.STRENGTH);
        if (org.bukkit.potion.PotionEffectType.getByName("foton:unknown") != null) {
            throw new AssertionError("unknown potion lookup must return null");
        }
    }

    private static void assertCanonical(String name,
            org.bukkit.potion.PotionEffectType canonical) {
        if (org.bukkit.potion.PotionEffectType.getByName(name) != canonical) {
            throw new AssertionError(
                "potion lookup by name did not return the canonical instance for " + name);
        }
        if (org.bukkit.potion.PotionEffectType.getById(canonical.getId()) != canonical) {
            throw new AssertionError(
                "potion lookup by id did not return the canonical instance for " + name);
        }
    }
}
