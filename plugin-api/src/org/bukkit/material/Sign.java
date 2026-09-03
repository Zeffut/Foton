package org.bukkit.material;

import org.bukkit.Material;
import org.bukkit.block.BlockFace;

/** Legacy sign material data compatible with Bukkit's pre-block-data API. */
@Deprecated
public class Sign extends MaterialData {
    public Sign() { super(Material.matchMaterial("OAK_SIGN")); }
    public Sign(Material type) { super(type); }
    public Sign(Material type, byte data) { super(type, data); }

    /** Returns the wall attachment direction or the closest cardinal direction. */
    public BlockFace getFacing() {
        switch (getData() & 0xF) {
            case 2: return BlockFace.NORTH;
            case 3: return BlockFace.SOUTH;
            case 4: return BlockFace.WEST;
            case 5: return BlockFace.EAST;
            default: return BlockFace.NORTH;
        }
    }

    public void setFacingDirection(BlockFace face) {
        if (face == null) return;
        byte value = (byte) switch (face) {
            case NORTH -> 2;
            case SOUTH -> 3;
            case WEST -> 4;
            case EAST -> 5;
            default -> getData() & 0xF;
        };
        setData(value);
    }
}
