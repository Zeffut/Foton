package org.bukkit.entity;

/** A lingering area-of-effect cloud. */
public interface AreaEffectCloud extends Entity {
    org.bukkit.projectiles.ProjectileSource getSource();
    org.bukkit.potion.PotionType getBasePotionType();
    float getRadius();
    void setRadius(float radius);
    int getDuration();
    void setDuration(int duration);
    int getWaitTime();
    void setWaitTime(int ticks);
    int getReapplicationDelay();
    void setReapplicationDelay(int ticks);
    float getRadiusPerTick();
    void setRadiusPerTick(float amount);
    float getRadiusOnUse();
    void setRadiusOnUse(float amount);

    /** Returns the custom effects currently carried by this cloud. */
    default java.util.Collection<org.bukkit.potion.PotionEffect> getEffects() {
        String[] values = foton.Native.areaEffectCloudEffects(getUniqueId().toString());
        java.util.ArrayList<org.bukkit.potion.PotionEffect> result = new java.util.ArrayList<>();
        if (values == null) return result;
        for (String value : values) {
            if (value == null) continue;
            String[] parts = value.split("\\|", -1);
            if (parts.length < 6) continue;
            String name = parts[0];
            int separator = name.indexOf(':');
            if (separator >= 0) name = name.substring(separator + 1);
            try {
                org.bukkit.potion.PotionEffectType type = org.bukkit.potion.PotionEffectType.getByName(name);
                if (type != null) result.add(new org.bukkit.potion.PotionEffect(type, Integer.parseInt(parts[1]), Integer.parseInt(parts[2]), Boolean.parseBoolean(parts[3]), Boolean.parseBoolean(parts[4]), Boolean.parseBoolean(parts[5])));
            } catch (NumberFormatException ignored) { }
        }
        return result;
    }

    /** Adds an effect, replacing an existing effect of the same type when requested. */
    boolean addCustomEffect(org.bukkit.potion.PotionEffect effect, boolean override);

    /** Removes all custom effects from this cloud. */
    void clearCustomEffects();

    /** Bukkit-compatible list view of the custom effects. */
    default java.util.List<org.bukkit.potion.PotionEffect> getCustomEffects() { return new java.util.ArrayList<>(getEffects()); }

    /** Returns whether this cloud carries at least one custom effect. */
    default boolean hasCustomEffects() { return !getEffects().isEmpty(); }
}
