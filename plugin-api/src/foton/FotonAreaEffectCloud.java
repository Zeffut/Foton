package foton;

import java.util.UUID;

/** Live Bukkit view of a Steel area-effect cloud. */
public final class FotonAreaEffectCloud extends FotonEntity implements org.bukkit.entity.AreaEffectCloud {
    public FotonAreaEffectCloud(UUID id) { super(id); }

    @Override public boolean addCustomEffect(org.bukkit.potion.PotionEffect effect, boolean override) {
        if (effect == null || effect.getType() == null) return false;
        return Native.addAreaEffectCloudEffect(getUniqueId().toString(), effect.getType().getName(), effect.getDuration(), effect.getAmplifier(), effect.isAmbient(), effect.hasParticles(), effect.hasIcon(), override);
    }
    @Override public void clearCustomEffects() { Native.clearAreaEffectCloudEffects(getUniqueId().toString()); }

    @Override public float getRadius() {
        return Native.areaEffectCloudRadius(getUniqueId().toString());
    }

    @Override public void setRadius(float radius) { Native.setAreaEffectCloudRadius(getUniqueId().toString(), radius); }
    @Override public int getDuration() { return Native.areaEffectCloudDuration(getUniqueId().toString()); }
    @Override public void setDuration(int ticks) { Native.setAreaEffectCloudDuration(getUniqueId().toString(), ticks); }
    @Override public int getWaitTime() { return Native.areaEffectCloudWaitTime(getUniqueId().toString()); }
    @Override public void setWaitTime(int ticks) { Native.setAreaEffectCloudWaitTime(getUniqueId().toString(), ticks); }
    @Override public int getReapplicationDelay() { return Native.areaEffectCloudReapplicationDelay(getUniqueId().toString()); }
    @Override public void setReapplicationDelay(int ticks) { Native.setAreaEffectCloudReapplicationDelay(getUniqueId().toString(), ticks); }
    @Override public float getRadiusPerTick() { return Native.areaEffectCloudRadiusPerTick(getUniqueId().toString()); }
    @Override public void setRadiusPerTick(float amount) { Native.setAreaEffectCloudRadiusPerTick(getUniqueId().toString(), amount); }
    @Override public float getRadiusOnUse() { return Native.areaEffectCloudRadiusOnUse(getUniqueId().toString()); }
    @Override public void setRadiusOnUse(float amount) { Native.setAreaEffectCloudRadiusOnUse(getUniqueId().toString(), amount); }
    @Override public org.bukkit.potion.PotionType getBasePotionType() {
        String value = Native.areaEffectCloudBasePotionType(getUniqueId().toString());
        if (value == null) return null;
        try { return org.bukkit.potion.PotionType.valueOf(value); } catch (IllegalArgumentException ignored) { return null; }
    }

    @Override public org.bukkit.projectiles.ProjectileSource getSource() {
        String source = Native.areaEffectCloudSource(getUniqueId().toString());
        try { return source == null ? null : (org.bukkit.projectiles.ProjectileSource) FotonEntity.handle(UUID.fromString(source)); } catch (IllegalArgumentException e) { return null; }
    }
}
