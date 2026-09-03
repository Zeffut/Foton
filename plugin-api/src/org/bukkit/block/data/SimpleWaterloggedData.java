package org.bukkit.block.data;

/** Vanilla waterlogged block data backed by Foton's state string. */
public final class SimpleWaterloggedData extends SimpleBlockData implements Waterlogged {
    public SimpleWaterloggedData(String text) { super(text); }
    @Override public boolean isWaterlogged() { return property("waterlogged"); }
    @Override public void setWaterlogged(boolean value) { property("waterlogged", value); }
}
