package org.bukkit;

/** Something that can carry a custom name. */
public interface Nameable {
    /** The custom name, or null when there is none. */
    net.kyori.adventure.text.Component customName();

    /** Sets the custom name; null removes it. */
    void customName(net.kyori.adventure.text.Component customName);

    String getCustomName();

    void setCustomName(String name);
}
