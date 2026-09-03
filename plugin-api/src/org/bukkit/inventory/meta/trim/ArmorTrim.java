package org.bukkit.inventory.meta.trim;

/** Immutable armor trim selection. */
public final class ArmorTrim {
    private final TrimMaterial material;
    private final TrimPattern pattern;
    public ArmorTrim(TrimMaterial material, TrimPattern pattern) {
        if (material == null || pattern == null) throw new IllegalArgumentException("trim material and pattern are required");
        this.material = material;
        this.pattern = pattern;
    }
    public TrimMaterial getMaterial() { return material; }
    public TrimPattern getPattern() { return pattern; }
    @Override public boolean equals(Object other) {
        return other instanceof ArmorTrim trim && material.equals(trim.material) && pattern.equals(trim.pattern);
    }
    @Override public int hashCode() { return 31 * material.hashCode() + pattern.hashCode(); }
}
