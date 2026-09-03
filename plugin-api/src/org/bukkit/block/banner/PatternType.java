package org.bukkit.block.banner;

/** Registry-backed banner pattern key. */
public final class PatternType {
    private final String key;
    private PatternType(String key) { this.key = key; }
    public static PatternType of(String key) { return key == null ? null : new PatternType(key); }
    public static PatternType getByIdentifier(String key) { return of(key); }
    public String getIdentifier() { return key; }
    @Override public boolean equals(Object value) { return value instanceof PatternType other && key.equals(other.key); }
    @Override public int hashCode() { return key.hashCode(); }
    @Override public String toString() { return key; }
}
