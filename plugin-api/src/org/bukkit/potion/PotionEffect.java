package org.bukkit.potion;

/** One active mob effect instance. */
public final class PotionEffect {
    private final PotionEffectType type;
    private final int duration;
    private final int amplifier;
    private final boolean ambient;
    private final boolean particles;
    private final boolean icon;

    public PotionEffect(java.util.Map<String, Object> data) {
        this(PotionEffectType.getByName(String.valueOf(data.get("effect"))),
            ((Number) data.getOrDefault("duration", 0)).intValue(),
            ((Number) data.getOrDefault("amplifier", 0)).intValue(),
            Boolean.TRUE.equals(data.get("ambient")),
            !Boolean.FALSE.equals(data.get("has-particles")),
            !Boolean.FALSE.equals(data.get("has-icon")));
    }

    public PotionEffect(PotionEffectType type, int duration, int amplifier) {
        this(type, duration, amplifier, false, true, true);
    }
    public PotionEffect(PotionEffectType type, int duration, int amplifier, boolean ambient) {
        this(type, duration, amplifier, ambient, true, true);
    }
    public PotionEffect(PotionEffectType type, int duration, int amplifier,
            boolean ambient, boolean particles, boolean icon) {
        this.type = type;
        this.duration = duration;
        this.amplifier = amplifier;
        this.ambient = ambient;
        this.particles = particles;
        this.icon = icon;
    }
    public PotionEffectType getType() { return type; }
    public int getDuration() { return duration; }
    public int getAmplifier() { return amplifier; }
    public boolean isAmbient() { return ambient; }
    public boolean hasParticles() { return particles; }
    public boolean hasIcon() { return icon; }
    public boolean apply(org.bukkit.entity.LivingEntity entity) {
        return entity != null && entity.addPotionEffect(this);
    }
    public PotionEffect clone() { return new PotionEffect(type, duration, amplifier, ambient, particles, icon); }
    /** Serializes this effect using Bukkit configuration keys. */
    public java.util.Map<String, Object> serialize() {
        java.util.Map<String, Object> values = new java.util.LinkedHashMap<>();
        values.put("effect", type.getName());
        values.put("duration", duration);
        values.put("amplifier", amplifier);
        values.put("ambient", ambient);
        values.put("has-particles", particles);
        values.put("has-icon", icon);
        return values;
    }
    @Override public boolean equals(Object other) {
        return other instanceof PotionEffect effect && type.equals(effect.type)
            && duration == effect.duration && amplifier == effect.amplifier
            && ambient == effect.ambient && particles == effect.particles && icon == effect.icon;
    }
    @Override public int hashCode() { return java.util.Objects.hash(type, duration, amplifier, ambient, particles, icon); }
}
