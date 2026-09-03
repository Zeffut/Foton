package org.bukkit.block.data.type;

/** Text-backed adapter for vanilla door block data. */
public final class SimpleDoorData extends org.bukkit.block.data.SimpleBisectedData implements Door {
    @Override public org.bukkit.block.BlockFace getFacing() {
        String value = propertyValue("facing");
        try { return org.bukkit.block.BlockFace.valueOf(value.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return org.bukkit.block.BlockFace.NORTH; }
    }
    @Override public void setFacing(org.bukkit.block.BlockFace facing) {
        if (facing == null || (facing != org.bukkit.block.BlockFace.NORTH && facing != org.bukkit.block.BlockFace.SOUTH && facing != org.bukkit.block.BlockFace.EAST && facing != org.bukkit.block.BlockFace.WEST)) throw new IllegalArgumentException("facing must be horizontal");
        property("facing", facing.name().toLowerCase(java.util.Locale.ROOT));
    }
    public SimpleDoorData(String text) { super(text); }
    @Override public Hinge getHinge() { return propertyValue("hinge").equalsIgnoreCase("right") ? Hinge.RIGHT : Hinge.LEFT; }
    @Override public void setHinge(Hinge hinge) { if (hinge != null) property("hinge", hinge.name().toLowerCase(java.util.Locale.ROOT)); }
    @Override public boolean isOpen() { return property("open"); }
    @Override public void setOpen(boolean open) { property("open", open); }
}
