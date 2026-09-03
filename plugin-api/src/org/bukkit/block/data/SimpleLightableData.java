package org.bukkit.block.data;

/** Property-backed implementation for vanilla lit block states. */
public final class SimpleLightableData extends SimpleBlockData implements Lightable {
    public SimpleLightableData(String text) { super(text); }
    @Override public boolean isLit() { return property("lit"); }
    @Override public void setLit(boolean lit) { property("lit", lit); }
}
