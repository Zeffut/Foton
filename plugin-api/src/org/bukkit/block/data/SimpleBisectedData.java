package org.bukkit.block.data;

/** Text-backed adapter for upper/lower block halves. */
public class SimpleBisectedData extends SimpleBlockData implements Bisected {
    public SimpleBisectedData(String text) { super(text); }
    @Override public Half getHalf() {
        int start = text.indexOf("[half=");
        if (start < 0) return Half.BOTTOM;
        int end = text.indexOf(']', start);
        String value = text.substring(start + 6, end < 0 ? text.length() : end);
        return "upper".equalsIgnoreCase(value) ? Half.TOP : Half.BOTTOM;
    }
    @Override public void setHalf(Half half) {
        if (half == null) throw new IllegalArgumentException("half cannot be null");
        property("half", half == Half.TOP ? "upper" : "lower");
    }
}
