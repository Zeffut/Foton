package org.bukkit.potion;

/** One active mob-effect instance. */
@org.bukkit.configuration.serialization.SerializableAs("PotionEffect")
public class PotionEffect implements org.bukkit.configuration.serialization.ConfigurationSerializable {
    public static final int INFINITE_DURATION = -1;

    private final PotionEffectType type;
    private final int duration;
    private final int amplifier;
    private final boolean ambient;
    private final boolean particles;
    private final boolean icon;
    private final PotionEffect hiddenEffect;

    @org.jetbrains.annotations.ApiStatus.Internal
    public PotionEffect(@org.jetbrains.annotations.NotNull PotionEffectType type,
            int duration, int amplifier, boolean ambient, boolean particles, boolean icon,
            @org.jetbrains.annotations.Nullable PotionEffect hiddenEffect) {
        if (type == null) throw new IllegalArgumentException("effect type cannot be null");
        this.type = type;
        this.duration = duration;
        this.amplifier = amplifier;
        this.ambient = ambient;
        this.particles = particles;
        this.icon = icon;
        this.hiddenEffect = hiddenEffect;
    }

    public PotionEffect(@org.jetbrains.annotations.NotNull PotionEffectType type,
            int duration, int amplifier,
            boolean ambient, boolean particles, boolean icon) {
        this(type, duration, amplifier, ambient, particles, icon, null);
    }

    public PotionEffect(@org.jetbrains.annotations.NotNull PotionEffectType type,
            int duration, int amplifier,
            boolean ambient, boolean particles) {
        this(type, duration, amplifier, ambient, particles, particles);
    }

    public PotionEffect(@org.jetbrains.annotations.NotNull PotionEffectType type,
            int duration, int amplifier, boolean ambient) {
        this(type, duration, amplifier, ambient, true);
    }

    public PotionEffect(@org.jetbrains.annotations.NotNull PotionEffectType type,
            int duration, int amplifier) {
        this(type, duration, amplifier, true);
    }

    public PotionEffect(@org.jetbrains.annotations.NotNull java.util.Map<String, Object> data) {
        this(effectType(data), integer(data, "duration"), integer(data, "amplifier"),
            bool(data, "ambient", false), bool(data, "has-particles", true),
            bool(data, "has-icon", bool(data, "has-particles", true)),
            (PotionEffect) data.get("hidden_effect"));
    }

    private static PotionEffectType effectType(java.util.Map<?, ?> data) {
        Object raw = data.get("effect");
        PotionEffectType type;
        if (raw instanceof String name) {
            type = PotionEffectType.getByKey(org.bukkit.NamespacedKey.fromString(name));
        } else {
            type = PotionEffectType.getById(integer(data, "effect"));
        }
        if (type == null) throw new java.util.NoSuchElementException(data + " does not contain effect");
        return type;
    }

    private static int integer(java.util.Map<?, ?> data, Object key) {
        Object value = data.get(key);
        if (value instanceof Integer number) return number;
        throw new java.util.NoSuchElementException(data + " does not contain " + key);
    }

    private static boolean bool(java.util.Map<?, ?> data, Object key, boolean fallback) {
        Object value = data.get(key);
        return value instanceof Boolean flag ? flag : fallback;
    }

    @org.jetbrains.annotations.NotNull
    public PotionEffect withType(@org.jetbrains.annotations.NotNull PotionEffectType value) {
        return new PotionEffect(value, duration, amplifier, ambient, particles, icon);
    }

    @org.jetbrains.annotations.NotNull
    public PotionEffect withDuration(int value) {
        return new PotionEffect(type, value, amplifier, ambient, particles, icon);
    }

    @org.jetbrains.annotations.NotNull
    public PotionEffect withAmplifier(int value) {
        return new PotionEffect(type, duration, value, ambient, particles, icon);
    }

    @org.jetbrains.annotations.NotNull
    public PotionEffect withAmbient(boolean value) {
        return new PotionEffect(type, duration, amplifier, value, particles, icon);
    }

    @org.jetbrains.annotations.NotNull
    public PotionEffect withParticles(boolean value) {
        return new PotionEffect(type, duration, amplifier, ambient, value, icon);
    }

    @org.jetbrains.annotations.NotNull
    public PotionEffect withIcon(boolean value) {
        return new PotionEffect(type, duration, amplifier, ambient, particles, value);
    }

    @org.jetbrains.annotations.Nullable
    public PotionEffect getHiddenPotionEffect() { return hiddenEffect; }

    @Override
    @org.jetbrains.annotations.NotNull
    public java.util.Map<String, Object> serialize() {
        java.util.Map<String, Object> values = new java.util.LinkedHashMap<>();
        values.put("effect", type.getKey().toString());
        values.put("duration", duration);
        values.put("amplifier", amplifier);
        values.put("ambient", ambient);
        values.put("has-particles", particles);
        values.put("has-icon", icon);
        if (hiddenEffect != null) values.put("hidden_effect", hiddenEffect);
        return java.util.Collections.unmodifiableMap(values);
    }

    public boolean apply(@org.jetbrains.annotations.NotNull org.bukkit.entity.LivingEntity entity) {
        return entity.addPotionEffect(this);
    }

    @Override public boolean equals(Object other) {
        return other == this || other instanceof PotionEffect effect
            && type.equals(effect.type)
            && ambient == effect.ambient
            && amplifier == effect.amplifier
            && duration == effect.duration
            && particles == effect.particles
            && icon == effect.icon
            && java.util.Objects.equals(hiddenEffect, effect.hiddenEffect);
    }

    public int getAmplifier() { return amplifier; }
    public int getDuration() { return duration; }
    public boolean isInfinite() { return duration == INFINITE_DURATION; }
    public boolean isShorterThan(@org.jetbrains.annotations.NotNull PotionEffect other) {
        return !isInfinite() && (duration < other.duration || other.isInfinite());
    }
    @org.jetbrains.annotations.NotNull
    public PotionEffectType getType() { return type; }
    public boolean isAmbient() { return ambient; }
    public boolean hasParticles() { return particles; }
    @Deprecated(since = "1.13")
    @org.jetbrains.annotations.Nullable
    @org.jetbrains.annotations.Contract("-> null")
    public org.bukkit.Color getColor() { return null; }
    public boolean hasIcon() { return icon; }

    @Override public int hashCode() {
        int hash = 1;
        hash = hash * 31 + type.hashCode();
        hash = hash * 31 + amplifier;
        hash = hash * 31 + duration;
        hash ^= 0x22222222 >> (ambient ? 1 : -1);
        hash ^= 0x22222222 >> (particles ? 1 : -1);
        hash ^= 0x22222222 >> (icon ? 1 : -1);
        if (hiddenEffect != null) hash = hash * 31 + hiddenEffect.hashCode();
        return hash;
    }

    @Override public String toString() {
        return "PotionEffect{amplifier=" + amplifier + ", duration=" + duration + ", type="
            + type + ", ambient=" + ambient + ", particles=" + particles + ", icon=" + icon
            + ", hiddenEffect=" + hiddenEffect + "}";
    }
}
