import java.lang.reflect.Field;
import java.lang.reflect.Modifier;

/** Verifies that legacy potion lookups return the canonical static instances. */
public final class FotonPotionLookupRunner {
    private FotonPotionLookupRunner() {}

    public static void main(String[] args) throws IllegalAccessException {
        int checkedEffects = 0;
        java.util.Set<org.bukkit.potion.PotionEffectType> identities =
            java.util.Collections.newSetFromMap(new java.util.IdentityHashMap<>());
        for (Field field : org.bukkit.potion.PotionEffectType.class.getFields()) {
            if (field.getType() != org.bukkit.potion.PotionEffectType.class
                    || !Modifier.isStatic(field.getModifiers())) {
                continue;
            }
            org.bukkit.potion.PotionEffectType canonical =
                (org.bukkit.potion.PotionEffectType) field.get(null);
            assertCanonical(canonical.getKey().getKey(), canonical);
            assertCanonical(canonical.getKey().toString(), canonical);
            if (org.bukkit.potion.PotionEffectType.getById(canonical.getId()) != canonical) {
                throw new AssertionError(
                    "potion lookup by id did not return the canonical instance for " + field.getName());
            }
            checkedEffects++;
            identities.add(canonical);
        }
        if (checkedEffects != 49 || identities.size() != 40
                || org.bukkit.potion.PotionEffectType.values().length != 40) {
            throw new AssertionError("expected 40 canonical potion effects and nine alias fields");
        }
        if (org.bukkit.potion.PotionEffectType.getByName("foton:unknown") != null) {
            throw new AssertionError("unknown potion lookup must return null");
        }
        if (org.bukkit.potion.PotionEffectType.getByName("foton:haste") != null) {
            throw new AssertionError("foreign namespaces must not resolve vanilla potion effects");
        }
    }

    private static void assertCanonical(String name,
            org.bukkit.potion.PotionEffectType canonical) {
        if (org.bukkit.potion.PotionEffectType.getByName(name) != canonical) {
            throw new AssertionError(
                "potion lookup by name did not return the canonical instance for " + name);
        }
    }
}
