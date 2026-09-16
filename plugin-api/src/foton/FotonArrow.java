package foton;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.UUID;
import org.bukkit.entity.Arrow;
import org.bukkit.potion.PotionEffect;
import org.bukkit.potion.PotionEffectType;

/** Arrow handle backed by the arrow's server-side mob-effect list. */
public class FotonArrow extends FotonProjectile implements Arrow {
    public FotonArrow(UUID id) { super(id); }

    @Override public void setBasePotionType(org.bukkit.potion.PotionType type) {
        Native.setArrowPotion(getUniqueId().toString(),
            type == null ? "" : "minecraft:" + type.name().toLowerCase(java.util.Locale.ROOT));
    }

    @Override public void setBasePotionData(org.bukkit.potion.PotionData data) {
        if (data == null) {
            setBasePotionType(null);
            return;
        }
        String name = data.getType().name();
        if (data.isExtended() && !name.startsWith("LONG_")) name = "LONG_" + name;
        if (data.isUpgraded() && !name.startsWith("STRONG_")) name = "STRONG_" + name;
        try { setBasePotionType(org.bukkit.potion.PotionType.valueOf(name)); }
        catch (IllegalArgumentException error) {
            throw new IllegalArgumentException("Unsupported legacy potion data: " + name, error);
        }
    }

    @Override public org.bukkit.potion.PotionType getBasePotionType() {
        String value = Native.arrowPotion(getUniqueId().toString());
        if (value == null) return null;
        String name = value.substring(value.indexOf(':') + 1).toUpperCase(java.util.Locale.ROOT);
        try { return org.bukkit.potion.PotionType.valueOf(name); }
        catch (IllegalArgumentException ignored) { return null; }
    }

    @Override public org.bukkit.potion.PotionData getBasePotionData() {
        org.bukkit.potion.PotionType type = getBasePotionType();
        if (type == null) return null;
        String name = type.name();
        boolean extended = name.startsWith("LONG_");
        boolean upgraded = name.startsWith("STRONG_");
        if (extended) name = name.substring(5);
        if (upgraded) name = name.substring(7);
        try { return new org.bukkit.potion.PotionData(org.bukkit.potion.PotionType.valueOf(name), extended, upgraded); }
        catch (IllegalArgumentException ignored) { return null; }
    }

    @Override public org.bukkit.Color getColor() {
        int raw = Native.arrowPotionColor(getUniqueId().toString());
        return raw == -1 ? null : org.bukkit.Color.fromARGB(raw);
    }

    @Override public void setColor(org.bukkit.Color color) {
        Native.setArrowPotionColor(getUniqueId().toString(), color == null ? 0 : color.asARGB(),
            color != null);
    }

    @Override public List<PotionEffect> getCustomEffects() {
        String[] encoded = Native.arrowCustomEffects(getUniqueId().toString());
        if (encoded == null) return List.of();
        ArrayList<PotionEffect> result = new ArrayList<>();
        for (String value : encoded) {
            String[] fields = value.split("\\|", -1);
            if (fields.length != 6) continue;
            try {
                PotionEffectType type = PotionEffectType.getByName(fields[0]);
                if (type != null) result.add(new PotionEffect(type,
                    Integer.parseInt(fields[1]), Integer.parseInt(fields[2]),
                    Boolean.parseBoolean(fields[3]), Boolean.parseBoolean(fields[4]),
                    Boolean.parseBoolean(fields[5])));
            } catch (NumberFormatException ignored) { }
        }
        return Collections.unmodifiableList(result);
    }

    @Override public boolean hasCustomEffects() { return !getCustomEffects().isEmpty(); }

    @Override public boolean addCustomEffect(PotionEffect effect, boolean overwrite) {
        java.util.Objects.requireNonNull(effect, "effect");
        return Native.addArrowCustomEffect(getUniqueId().toString(), effect.getType().getName(),
            effect.getDuration(), effect.getAmplifier(), effect.isAmbient(), effect.hasParticles(),
            effect.hasIcon(), overwrite);
    }

    @Override public boolean removeCustomEffect(PotionEffectType type) {
        java.util.Objects.requireNonNull(type, "type");
        return Native.removeArrowCustomEffect(getUniqueId().toString(), type.getName());
    }

    @Override public boolean hasCustomEffect(PotionEffectType type) {
        if (type == null) return false;
        return getCustomEffects().stream().anyMatch(effect -> effect.getType().equals(type));
    }

    @Override public void clearCustomEffects() {
        Native.clearArrowCustomEffects(getUniqueId().toString());
    }

    private String property(String name) {
        return Native.arrowProperty(getUniqueId().toString(), name);
    }

    private void setProperty(String name, String value) {
        if (!Native.setArrowProperty(getUniqueId().toString(), name, value)) {
            throw new IllegalArgumentException("Invalid arrow " + name + ": " + value);
        }
    }

    @Override public int getKnockbackStrength() { return 0; }
    @Override public void setKnockbackStrength(int knockbackStrength) { }
    @Override public double getDamage() { return Double.parseDouble(property("damage")); }
    @Override public void setDamage(double damage) {
        if (!(damage >= 0.0)) throw new IllegalArgumentException("damage");
        setProperty("damage", Double.toString(damage));
    }
    @Override public int getPierceLevel() { return Integer.parseInt(property("pierce")); }
    @Override public void setPierceLevel(int pierceLevel) {
        if (pierceLevel < 0 || pierceLevel > 127) throw new IllegalArgumentException("pierceLevel");
        setProperty("pierce", Integer.toString(pierceLevel));
    }
    @Override public boolean isCritical() { return Boolean.parseBoolean(property("critical")); }
    @Override public void setCritical(boolean critical) {
        setProperty("critical", Boolean.toString(critical));
    }
    @Override public boolean isInBlock() { return Boolean.parseBoolean(property("in_block")); }
    @Override public org.bukkit.block.Block getAttachedBlock() {
        java.util.List<org.bukkit.block.Block> blocks = getAttachedBlocks();
        return blocks.isEmpty() ? null : blocks.get(0);
    }
    @Override public java.util.List<org.bukkit.block.Block> getAttachedBlocks() {
        org.bukkit.World world = getWorld();
        String encoded = property("attached");
        if (world == null || encoded == null || encoded.isEmpty()) return java.util.List.of();
        java.util.ArrayList<org.bukkit.block.Block> blocks = new java.util.ArrayList<>();
        for (String value : encoded.split(";")) {
            String[] fields = value.split(",", -1);
            if (fields.length != 3) continue;
            try {
                blocks.add(world.getBlockAt(Integer.parseInt(fields[0]),
                    Integer.parseInt(fields[1]), Integer.parseInt(fields[2])));
            } catch (NumberFormatException ignored) { }
        }
        return java.util.Collections.unmodifiableList(blocks);
    }
    @Override public org.bukkit.entity.AbstractArrow.PickupStatus getPickupStatus() {
        return org.bukkit.entity.AbstractArrow.PickupStatus.valueOf(property("pickup"));
    }
    @Override public void setPickupStatus(org.bukkit.entity.AbstractArrow.PickupStatus status) {
        setProperty("pickup", java.util.Objects.requireNonNull(status, "status").name());
    }
    @Override public boolean isShotFromCrossbow() {
        return Boolean.parseBoolean(property("crossbow"));
    }
    @Override public void setShotFromCrossbow(boolean shotFromCrossbow) { }
    @Override public org.bukkit.inventory.ItemStack getItem() { return getItemStack(); }
    @Override public void setItem(org.bukkit.inventory.ItemStack item) { setItemStack(item); }
    @Override public org.bukkit.inventory.ItemStack getWeapon() {
        String encoded = property("weapon");
        if (encoded == null) return null;
        if (encoded.isEmpty()) return new org.bukkit.inventory.ItemStack(org.bukkit.Material.AIR);
        return FotonInventory.decode(encoded);
    }
    @Override public void setWeapon(org.bukkit.inventory.ItemStack item) {
        setProperty("weapon", FotonInventory.encode(java.util.Objects.requireNonNull(item, "item")));
    }
    @Override public org.bukkit.inventory.ItemStack getItemStack() {
        String encoded = property("item");
        return encoded == null || encoded.isEmpty()
            ? new org.bukkit.inventory.ItemStack(org.bukkit.Material.AIR)
            : FotonInventory.decode(encoded);
    }
    @Override public void setItemStack(org.bukkit.inventory.ItemStack stack) {
        setProperty("item", FotonInventory.encode(java.util.Objects.requireNonNull(stack, "stack")));
    }
    @Override public void setLifetimeTicks(int ticks) {
        setProperty("lifetime", Integer.toString(ticks));
    }
    @Override public int getLifetimeTicks() { return Integer.parseInt(property("lifetime")); }
    @Override public org.bukkit.Sound getHitSound() { return org.bukkit.Sound.match(property("sound")); }
    @Override public void setHitSound(org.bukkit.Sound sound) {
        setProperty("sound", java.util.Objects.requireNonNull(sound, "sound").getKey().toString());
    }
    @Override public void setShooter(org.bukkit.projectiles.ProjectileSource source) {
        setShooter(source, true);
    }
    @Override public void setShooter(
            org.bukkit.projectiles.ProjectileSource source, boolean resetPickupStatus) {
        String shooter = source instanceof org.bukkit.entity.Entity entity
            ? entity.getUniqueId().toString() : "";
        Native.setArrowShooter(getUniqueId().toString(), shooter, resetPickupStatus);
    }
}
