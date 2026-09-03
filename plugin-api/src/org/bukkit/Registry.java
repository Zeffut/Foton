package org.bukkit;

import java.util.Arrays;
import java.util.Objects;
import java.util.stream.Stream;
import org.bukkit.enchantments.Enchantment;

/** Read-only Bukkit registry view backed by Steel's generated values. */
public interface Registry<T extends Keyed> {
    T get(NamespacedKey key);

    /** Returns the value or throws when the key is absent. */
    default T getOrThrow(NamespacedKey key) {
        return Objects.requireNonNull(get(key), "No registry value for " + key);
    }
    Stream<T> stream();

    default NamespacedKey getKey(T value) { return value == null ? null : value.getKey(); }

    default NamespacedKey getKeyOrThrow(T value) {
        return Objects.requireNonNull(value, "value").getKey();
    }

    Registry<org.bukkit.Art> ART = new Registry<>() {
        @Override public org.bukkit.Art get(NamespacedKey key) { if (key == null) return null; for (org.bukkit.Art value : org.bukkit.Art.values()) if (value.getKey().equals(key)) return value; return null; }
        @Override public Stream<org.bukkit.Art> stream() { return Arrays.stream(org.bukkit.Art.values()); }
    };

    Registry<org.bukkit.attribute.Attribute> ATTRIBUTE = new Registry<>() {
        @Override public org.bukkit.attribute.Attribute get(NamespacedKey key) { if (key == null) return null; for (org.bukkit.attribute.Attribute value : org.bukkit.attribute.Attribute.values()) if (value.getKey().equals(key)) return value; return null; }
        @Override public Stream<org.bukkit.attribute.Attribute> stream() { return Arrays.stream(org.bukkit.attribute.Attribute.values()); }
    };

    Registry<Material> MATERIAL = new Registry<>() {
        @Override public Material get(NamespacedKey key) { return key == null ? null : Material.matchMaterial(key.toString()); }
        @Override public Stream<Material> stream() { return Arrays.stream(Material.values()); }
    };

    Registry<org.bukkit.block.Biome> BIOME = new Registry<>() {
        @Override public org.bukkit.block.Biome get(NamespacedKey key) { if (key == null) return null; for (org.bukkit.block.Biome value : org.bukkit.block.Biome.values()) if (value.getKey().equals(key)) return value; return null; }
        @Override public Stream<org.bukkit.block.Biome> stream() { return Arrays.stream(org.bukkit.block.Biome.values()); }
    };

    Registry<Material> BLOCK = new Registry<>() {
        @Override public Material get(NamespacedKey key) { Material value = key == null ? null : Material.matchMaterial(key.toString()); return value != null && value.isBlock() ? value : null; }
        @Override public Stream<Material> stream() { return Arrays.stream(Material.values()).filter(Material::isBlock); }
    };
    Registry<Enchantment> ENCHANTMENT = new Registry<>() {
        @Override public Enchantment get(NamespacedKey key) {
            return Enchantment.getByKey(key);
        }
        @Override public Stream<Enchantment> stream() {
            return Arrays.stream(Enchantment.values());
        }
    };
    Registry<Particle> PARTICLE_TYPE = new Registry<>() {
        @Override public Particle get(NamespacedKey key) { return key == null ? null : java.util.Arrays.stream(Particle.values()).filter(p -> p.getKey().equals(key)).findFirst().orElse(null); }
        @Override public Stream<Particle> stream() { return Arrays.stream(Particle.values()); }
    };
    Registry<org.bukkit.inventory.meta.trim.TrimPattern> TRIM_PATTERN = keyedTrimPatterns();
    Registry<org.bukkit.inventory.meta.trim.TrimMaterial> TRIM_MATERIAL = keyedTrimMaterials();

    private static Registry<org.bukkit.inventory.meta.trim.TrimPattern> keyedTrimPatterns() {
        return keyedRegistry(new String[]{"sentry", "dune", "coast", "wild", "ward", "eye", "vex", "tide", "snout", "rib", "spire", "wayfinder", "shaper", "silence", "raiser", "host", "flow", "bolt"}, true);
    }
    private static Registry<org.bukkit.inventory.meta.trim.TrimMaterial> keyedTrimMaterials() {
        return keyedRegistry(new String[]{"quartz", "iron", "netherite", "redstone", "copper", "gold", "emerald", "diamond", "lapis", "amethyst", "resin"}, false);
    }
    @SuppressWarnings("unchecked")
    private static <T extends Keyed> Registry<T> keyedRegistry(String[] names, boolean pattern) {
        // The two call sites select the matching concrete wrapper for T.
        java.util.Map<NamespacedKey, T> values = new java.util.LinkedHashMap<>();
        for (String name : names) {
            T value = (T) (pattern ? new org.bukkit.inventory.meta.trim.TrimPattern(name) : new org.bukkit.inventory.meta.trim.TrimMaterial(name));
            values.put(value.getKey(), value);
        }
        return new Registry<>() {
            public T get(NamespacedKey key) { return values.get(key); }
            public Stream<T> stream() { return values.values().stream(); }
        };
    }
}
