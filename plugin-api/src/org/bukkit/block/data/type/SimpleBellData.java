package org.bukkit.block.data.type;

/** Text-backed adapter for bell facing and attachment. */
public final class SimpleBellData extends org.bukkit.block.data.SimpleDirectionalData implements Bell {
    public SimpleBellData(String text) { super(text); }
    @Override public Attachment getAttachment() {
        int start = text.indexOf("[attachment=");
        if (start < 0) return Attachment.FLOOR;
        int end = text.indexOf(']', start);
        String value = text.substring(start + 12, end < 0 ? text.length() : end);
        return switch (value.toLowerCase(java.util.Locale.ROOT)) {
            case "ceiling" -> Attachment.CEILING;
            case "single_wall" -> Attachment.SINGLE_WALL;
            case "double_wall" -> Attachment.DOUBLE_WALL;
            default -> Attachment.FLOOR;
        };
    }
    @Override public void setAttachment(Attachment attachment) {
        if (attachment == null) throw new IllegalArgumentException("attachment cannot be null");
        property("attachment", attachment.name().toLowerCase(java.util.Locale.ROOT));
    }
}
