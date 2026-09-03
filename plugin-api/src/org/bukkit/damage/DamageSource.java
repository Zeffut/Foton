package org.bukkit.damage;

public final class DamageSource {
    private final DamageType damageType;
    private DamageSource(DamageType damageType) { this.damageType = damageType; }
    public DamageType getDamageType() { return damageType; }
    public static Builder builder(DamageType type) { return new Builder(type); }
    public static final class Builder {
        private final DamageType type;
        private Builder(DamageType type) { this.type = type; }
        public DamageSource build() { return new DamageSource(type); }
    }
}
