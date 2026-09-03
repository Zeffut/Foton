package foton;

import java.util.UUID;
import org.bukkit.entity.LivingEntity;

/** Generic living-entity handle. */
public class FotonLivingEntity extends FotonEntity implements LivingEntity {
    public org.bukkit.entity.LivingEntity getTarget() {
        String target = Native.entityTarget(getUniqueId().toString());
        try { return target == null ? null : (org.bukkit.entity.LivingEntity) FotonEntity.handle(UUID.fromString(target)); }
        catch (IllegalArgumentException | ClassCastException ignored) { return null; }
    }
    public void setTarget(org.bukkit.entity.LivingEntity target) {
        Native.setEntityTarget(getUniqueId().toString(), target == null ? null : target.getUniqueId().toString());
    }

    @Override public org.bukkit.event.entity.EntityDamageEvent getLastDamageCause() {
        return EventBridge.lastDamageCause(getUniqueId());
    }
    @Override public void setLastDamageCause(org.bukkit.event.entity.EntityDamageEvent event) {
        EventBridge.setLastDamageCause(getUniqueId(), event);
    }
    @Override public boolean isHandRaised() { return Native.entityIsUsingItem(getUniqueId().toString()); }
    @Override public void clearActiveItem() { Native.entityClearActiveItem(getUniqueId().toString()); }
    @Override public org.bukkit.util.BoundingBox getBoundingBox() {
        double[] b = Native.entityBoundingBox(getUniqueId().toString());
        return b == null || b.length < 6 ? null : new org.bukkit.util.BoundingBox(b[0], b[1], b[2], b[3], b[4], b[5]);
    }
    @Override public boolean isInvisible() { return Native.entityInvisible(getUniqueId().toString()); }
    @Override public boolean getCanPickupItems() { return Native.entityCanPickupItems(getUniqueId().toString()); }
    @Override public boolean getRemoveWhenFarAway() { return Native.entityRemoveWhenFarAway(getUniqueId().toString()); }
    @Override public void setRemoveWhenFarAway(boolean remove) { Native.setEntityRemoveWhenFarAway(getUniqueId().toString(), remove); }
    @Override public boolean isPersistent() { return !getRemoveWhenFarAway(); }
    @Override public void setPersistent(boolean persistent) { setRemoveWhenFarAway(!persistent); }
    @Override public void setCanPickupItems(boolean pickup) { Native.setEntityCanPickupItems(getUniqueId().toString(), pickup); }
    @Override public boolean isCustomNameVisible() { return Native.entityCustomNameVisible(getUniqueId().toString()); }
    @Override public double getEyeHeight() { return Native.entityEyeHeight(getUniqueId().toString()); }
    @Override public int getNoDamageTicks() { return Native.entityNoDamageTicks(getUniqueId().toString()); }
    @Override public float getFallDistance() { return Native.entityFallDistance(getUniqueId().toString()); }
    @Override public void setFallDistance(float distance) { Native.setEntityFallDistance(getUniqueId().toString(), distance); }
    @Override public void setNoDamageTicks(int ticks) { Native.entitySetNoDamageTicks(getUniqueId().toString(), ticks); }
    @Override public int getFreezeTicks() { return Native.entityFreezeTicks(getUniqueId().toString()); }
    @Override public void setFreezeTicks(int ticks) { Native.setEntityFreezeTicks(getUniqueId().toString(), Math.max(0, ticks)); }
    @Override public int getMaxFreezeTicks() { return 140; }
    @Override public boolean isFrozen() { return getFreezeTicks() >= getMaxFreezeTicks(); }
    @Override public java.util.List<org.bukkit.entity.Entity> getNearbyEntities(double x, double y, double z) {
        String[] ids = Native.entityNearby(getUniqueId().toString(), x, y, z);
        java.util.ArrayList<org.bukkit.entity.Entity> result = new java.util.ArrayList<>();
        if (ids != null) for (String value : ids) try { result.add(FotonEntity.handle(UUID.fromString(value))); }
        catch (IllegalArgumentException ignored) { }
        return result;
    }
    @Override public org.bukkit.attribute.AttributeInstance getAttribute(org.bukkit.attribute.Attribute attribute) {
        if (attribute == null) return null;
        String encoded = Native.playerAttribute(getUniqueId().toString(), attribute.name());
        if (encoded == null) return null;
        String[] values = encoded.split("\\|", -1);
        if (values.length != 2) return null;
        try { return new org.bukkit.attribute.AttributeInstance(getUniqueId().toString(), attribute,
            Double.parseDouble(values[0]), Double.parseDouble(values[1])); }
        catch (NumberFormatException ignored) { return null; }
    }
    public FotonLivingEntity(UUID id) { super(id); }
    @Override public double getHealth() { return Native.health(getUniqueId().toString()); }
    @Override public void setHealth(double value) { Native.setHealth(getUniqueId().toString(), value); }
    @Override public double getMaxHealth() { return Native.maxHealth(getUniqueId().toString()); }
    @Override public void setMaxHealth(double value) { org.bukkit.attribute.AttributeInstance attribute = getAttribute(org.bukkit.attribute.Attribute.GENERIC_MAX_HEALTH); if (attribute != null) attribute.setBaseValue(value); }
    @Override public int getAir() { return Native.airSupply(getUniqueId().toString()); }
    @Override public void setAir(int ticks) { Native.setAirSupply(getUniqueId().toString(), ticks); }
    @Override public int getMaximumAir() { return Native.maxAirSupply(getUniqueId().toString()); }
    @Override public java.util.Collection<org.bukkit.potion.PotionEffect> getActivePotionEffects() {
        java.util.ArrayList<org.bukkit.potion.PotionEffect> result = new java.util.ArrayList<>();
        String[] encoded = Native.entityPotionEffects(getUniqueId().toString());
        if (encoded == null) return result;
        for (String value : encoded) {
            String[] fields = value.split("\\|", -1);
            if (fields.length != 6) continue;
            try {
                org.bukkit.potion.PotionEffectType type = org.bukkit.potion.PotionEffectType.getByName(fields[0]);
                if (type != null) result.add(new org.bukkit.potion.PotionEffect(type,
                    Integer.parseInt(fields[1]), Integer.parseInt(fields[2]),
                    Boolean.parseBoolean(fields[3]), Boolean.parseBoolean(fields[4]),
                    Boolean.parseBoolean(fields[5])));
            } catch (NumberFormatException ignored) { }
        }
        return java.util.Collections.unmodifiableList(result);
    }
    @Override public boolean hasPotionEffect(org.bukkit.potion.PotionEffectType type) { return getPotionEffect(type) != null; }
    @Override public org.bukkit.potion.PotionEffect getPotionEffect(org.bukkit.potion.PotionEffectType type) {
        if (type == null) return null;
        for (org.bukkit.potion.PotionEffect effect : getActivePotionEffects()) if (type.equals(effect.getType())) return effect;
        return null;
    }
    @Override public boolean addPotionEffect(org.bukkit.potion.PotionEffect effect) {
        if (effect == null || effect.getType() == null) return false;
        org.bukkit.potion.PotionEffect old = getPotionEffect(effect.getType());
        String action = old == null ? "ADDED" : "CHANGED";
        if (!EventBridge.firePotionEffect(getUniqueId().toString(), effect.getType().getName(),
                old == null ? -1 : old.getDuration(), old == null ? -1 : old.getAmplifier(),
                effect.getDuration(), effect.getAmplifier(), action)) return false;
        return Native.addPotionEffect(getUniqueId().toString(), effect.getType().getName(), effect.getDuration(), effect.getAmplifier());
    }
    @Override public void removePotionEffect(org.bukkit.potion.PotionEffectType type) {
        if (type == null) return;
        org.bukkit.potion.PotionEffect old = getPotionEffect(type);
        if (old == null) return;
        if (EventBridge.firePotionEffect(getUniqueId().toString(), type.getName(),
                old.getDuration(), old.getAmplifier(), -1, -1, "REMOVED"))
            Native.removePotionEffect(getUniqueId().toString(), type.getName());
    }
    @Override public org.bukkit.inventory.EntityEquipment getEquipment() {
        return new FotonEntityEquipment(getUniqueId().toString());
    }
}
