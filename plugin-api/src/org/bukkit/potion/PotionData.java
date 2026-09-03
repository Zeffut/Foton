package org.bukkit.potion;

/** Legacy Bukkit potion descriptor. */
public final class PotionData {
    private final PotionType type;
    private final boolean extended;
    private final boolean upgraded;
    public PotionData(PotionType type) { this(type, false, false); }
    public PotionData(PotionType type, boolean extended, boolean upgraded) {
        if (type == null) throw new IllegalArgumentException("type");
        if (extended && upgraded) throw new IllegalArgumentException("extended and upgraded cannot both be true");
        this.type = type; this.extended = extended; this.upgraded = upgraded;
    }
    public PotionType getType() { return type; }
    public boolean isExtended() { return extended; }
    public boolean isUpgraded() { return upgraded; }
    @Override public boolean equals(Object other) {
        return other instanceof PotionData data && type == data.type
            && extended == data.extended && upgraded == data.upgraded;
    }
    @Override public int hashCode() { return java.util.Objects.hash(type, extended, upgraded); }
}
