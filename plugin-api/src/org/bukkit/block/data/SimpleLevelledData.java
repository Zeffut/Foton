package org.bukkit.block.data;

/** Live textual adapter for a block state carrying a level property. */
public final class SimpleLevelledData extends SimpleBlockData implements Levelled {
    public SimpleLevelledData(String text) { super(text); }
    @Override public int getLevel() {
        int start = text.indexOf("[level=");
        if (start < 0) return 0;
        int end = text.indexOf(']', start);
        try { return Integer.parseInt(text.substring(start + 7, end < 0 ? text.length() : end)); }
        catch (NumberFormatException ignored) { return 0; }
    }
    @Override public void setLevel(int level) {
        if (level < 0 || level > getMaximumLevel()) throw new IllegalArgumentException("level out of range: " + level);
        property("level", Integer.toString(level));
    }
    @Override public int getMaximumLevel() { return 15; }
}
