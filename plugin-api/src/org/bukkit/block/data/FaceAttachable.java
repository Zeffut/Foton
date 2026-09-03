package org.bukkit.block.data;

/** Block data attached to a face. */
public interface FaceAttachable extends BlockData {
    enum AttachedFace { FLOOR, WALL, CEILING }
    AttachedFace getAttachedFace();
    void setAttachedFace(AttachedFace face);
}
