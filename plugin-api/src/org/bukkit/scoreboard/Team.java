package org.bukkit.scoreboard;

import java.util.Set;

/** Read-only scoreboard team view exposed to plugins. */
public interface Team {
    String getName();
    Set<String> getEntries();
}
