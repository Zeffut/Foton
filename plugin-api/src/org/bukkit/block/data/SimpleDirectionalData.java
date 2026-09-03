package org.bukkit.block.data;

import org.bukkit.Material;
import org.bukkit.block.BlockFace;

/** Directional block state backed by the vanilla state string. */
public class SimpleDirectionalData extends SimpleBlockData implements Directional {
    public SimpleDirectionalData(String text) { super(text); }

    @Override public BlockFace getFacing() {
        int start = text.indexOf("facing=");
        if (start < 0) return BlockFace.NORTH;
        int end = text.indexOf(',', start);
        if (end < 0) end = text.indexOf(']', start);
        String value = text.substring(start + 7, end < 0 ? text.length() : end).trim();
        try { return BlockFace.valueOf(value.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException error) { return BlockFace.NORTH; }
    }

    @Override public void setFacing(BlockFace facing) {
        if (facing != null) property("facing", facing.name().toLowerCase(java.util.Locale.ROOT));
    }
}
