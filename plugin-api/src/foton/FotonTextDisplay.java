package foton;

import java.util.Locale;
import java.util.UUID;
import org.bukkit.Color;

/** Live Bukkit view of a text display. */
public final class FotonTextDisplay extends FotonDisplay implements org.bukkit.entity.TextDisplay {
    // Vanilla's style flag bits, as the synced byte carries them.
    private static final int SHADOW = 1, SEE_THROUGH = 2, DEFAULT_BACKGROUND = 4, ALIGN_LEFT = 8, ALIGN_RIGHT = 16;

    public FotonTextDisplay(UUID id) { super(id); }

    private String id() { return getUniqueId().toString(); }

    private double state(int index, double otherwise) {
        double[] state = Native.textDisplayState(id());
        return state == null || state.length < 4 ? otherwise : state[index];
    }

    private int flags() { return (int) state(3, 0); }

    @Override public String getText() {
        String text = Native.textDisplayPlainText(id());
        return text == null ? "" : text;
    }
    @Override public void setText(String text) { text(net.kyori.adventure.text.Component.text(text == null ? "" : text)); }

    /** The text's content; the colours and formatting it was given reach
     * players but are not read back. */
    @Override public net.kyori.adventure.text.Component text() { return net.kyori.adventure.text.Component.text(getText()); }
    @Override public void text(net.kyori.adventure.text.Component text) {
        Native.setTextDisplayText(id(), FotonComponents.toJson(text == null ? net.kyori.adventure.text.Component.empty() : text));
    }

    @Override public int getLineWidth() { return (int) state(0, 200); }
    @Override public void setLineWidth(int width) { Native.setTextDisplayLineWidth(id(), width); }

    @Override public Color getBackgroundColor() {
        int color = (int) state(1, -1);
        return color == -1 ? null : Color.fromARGB(color);
    }
    @Override public void setBackgroundColor(Color color) {
        if (color == null) Native.resetTextDisplayBackground(id());
        else Native.setTextDisplayBackground(id(), color.asARGB());
    }

    @Override public byte getTextOpacity() { return (byte) state(2, -1); }
    @Override public void setTextOpacity(byte opacity) { Native.setTextDisplayOpacity(id(), opacity); }

    @Override public boolean isShadowed() { return (flags() & SHADOW) != 0; }
    @Override public void setShadowed(boolean shadow) { Native.setTextDisplayFlag(id(), "shadow", shadow); }
    @Override public boolean isSeeThrough() { return (flags() & SEE_THROUGH) != 0; }
    @Override public void setSeeThrough(boolean seeThrough) { Native.setTextDisplayFlag(id(), "see_through", seeThrough); }
    @Override public boolean isDefaultBackground() { return (flags() & DEFAULT_BACKGROUND) != 0; }
    @Override public void setDefaultBackground(boolean value) { Native.setTextDisplayFlag(id(), "default_background", value); }

    @Override public TextAlignment getAlignment() {
        int flags = flags();
        if ((flags & ALIGN_LEFT) != 0) return TextAlignment.LEFT;
        if ((flags & ALIGN_RIGHT) != 0) return TextAlignment.RIGHT;
        return TextAlignment.CENTER;
    }
    @Override public void setAlignment(TextAlignment alignment) {
        if (alignment == null) throw new IllegalArgumentException("Alignment cannot be null");
        Native.setTextDisplayAlignment(id(), alignment.name().toLowerCase(Locale.ROOT));
    }
}
