package org.bukkit.block.data;

/** Text-backed adapter for button/lever face attachment. */
public final class SimpleFaceAttachableData extends SimpleBlockData implements FaceAttachable {
    public SimpleFaceAttachableData(String text) { super(text); }
    @Override public AttachedFace getAttachedFace() {
        int start = text.indexOf("[face=");
        if (start < 0) return AttachedFace.WALL;
        int end = text.indexOf(']', start);
        String value = text.substring(start + 6, end < 0 ? text.length() : end);
        try { return AttachedFace.valueOf(value.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return AttachedFace.WALL; }
    }
    @Override public void setAttachedFace(AttachedFace face) {
        if (face == null) throw new IllegalArgumentException("face cannot be null");
        property("face", face.name().toLowerCase(java.util.Locale.ROOT));
    }
}
