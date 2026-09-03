package org.bukkit.potion;

import java.util.LinkedHashMap;
import java.util.Map;
import org.bukkit.Material;
import org.bukkit.configuration.serialization.ConfigurationSerializable;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.PotionMeta;

/** Legacy Bukkit potion descriptor backed by modern potion item metadata. */
public class Potion implements ConfigurationSerializable {
    private PotionType type;
    private int level;
    private boolean splash;
    private boolean extended;

    public Potion(PotionType type) { this(type, 1, false, false); }
    public Potion(PotionType type, int level) { this(type, level, false, false); }
    public Potion(PotionType type, int level, boolean splash) { this(type, level, splash, false); }
    public Potion(PotionType type, int level, boolean splash, boolean extended) {
        this.type = type == null ? PotionType.WATER : type;
        this.level = Math.max(1, level);
        this.splash = splash;
        this.extended = extended;
    }

    public PotionType getType() { return type; }
    public void setType(PotionType type) { if (type != null) this.type = type; }
    public int getLevel() { return level; }
    public void setLevel(int level) { this.level = Math.max(1, level); }
    public boolean isSplash() { return splash; }
    public void setSplash(boolean splash) { this.splash = splash; }
    public boolean hasExtendedDuration() { return extended; }
    public void setHasExtendedDuration(boolean extended) { this.extended = extended; }

    /** Returns the vanilla effects represented by this potion descriptor. */
    public java.util.Collection<PotionEffect> getEffects() {
        return type.createEffects(level);
    }

    /** Applies this descriptor to a potion item stack. */
    public void apply(ItemStack item) {
        if (item == null) throw new IllegalArgumentException("item");
        if (splash) {
            item.setType(Material.SPLASH_POTION);
        } else {
            item.setType(Material.POTION);
        }
        if (item.getItemMeta() instanceof PotionMeta meta) {
            meta.setBasePotionData(new PotionData(type, extended, level >= 2));
            item.setItemMeta(meta);
        }
    }

    public static Potion fromItemStack(ItemStack item) {
        if (item == null) throw new IllegalArgumentException("item");
        PotionType type = PotionType.WATER;
        boolean extended = false;
        int level = 1;
        if (item.getItemMeta() instanceof PotionMeta meta && meta.getBasePotionData() != null) {
            PotionData data = meta.getBasePotionData();
            type = data.getType();
            extended = data.isExtended();
            level = data.isUpgraded() ? 2 : 1;
        }
        Material material = item.getType();
        boolean splash = material == Material.SPLASH_POTION || material == Material.LINGERING_POTION;
        return new Potion(type, level, splash, extended);
    }

    @Override public Map<String, Object> serialize() {
        Map<String, Object> values = new LinkedHashMap<>();
        values.put("type", type.name());
        values.put("level", level);
        values.put("splash", splash);
        values.put("extended", extended);
        return values;
    }

    public static Potion deserialize(Map<String, Object> values) {
        if (values == null) throw new IllegalArgumentException("values");
        PotionType type;
        try { type = PotionType.valueOf(String.valueOf(values.getOrDefault("type", "WATER"))); }
        catch (IllegalArgumentException ignored) { type = PotionType.WATER; }
        int level = values.get("level") instanceof Number n ? n.intValue() : 1;
        return new Potion(type, level, Boolean.TRUE.equals(values.get("splash")),
            Boolean.TRUE.equals(values.get("extended")));
    }
}
