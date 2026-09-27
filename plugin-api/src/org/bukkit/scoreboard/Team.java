package org.bukkit.scoreboard;

import java.util.Set;

/** Read-only scoreboard team view exposed to plugins. */
public interface Team {
    String getName();
    Set<String> getEntries();
    void addEntry(String entry);
    boolean removeEntry(String entry);
    boolean hasEntry(String entry);
    net.kyori.adventure.text.Component prefix();
    void prefix(net.kyori.adventure.text.Component prefix);
    void unregister();
}
