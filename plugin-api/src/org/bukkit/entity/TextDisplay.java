package org.bukkit.entity;

import org.bukkit.Color;

/** A display entity rendering a component as text. */
public interface TextDisplay extends Display {
    String getText();
    void setText(String text);
    net.kyori.adventure.text.Component text();
    void text(net.kyori.adventure.text.Component text);

    /** The width, in pixels, lines wrap at. */
    int getLineWidth();
    void setLineWidth(int width);

    /** The background colour, or null for the default one. */
    Color getBackgroundColor();
    void setBackgroundColor(Color color);

    byte getTextOpacity();
    void setTextOpacity(byte opacity);

    boolean isShadowed();
    void setShadowed(boolean shadow);
    boolean isSeeThrough();
    void setSeeThrough(boolean seeThrough);
    boolean isDefaultBackground();
    void setDefaultBackground(boolean defaultBackground);

    TextAlignment getAlignment();
    void setAlignment(TextAlignment alignment);

    /** Where wrapped lines sit; each name is vanilla's, upper-cased. */
    enum TextAlignment {
        CENTER,
        LEFT,
        RIGHT
    }
}
