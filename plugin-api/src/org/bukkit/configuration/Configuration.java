package org.bukkit.configuration;

import java.util.Map;

/** A section that is nobody's child, and that can carry defaults. */
public interface Configuration extends ConfigurationSection {
    void addDefaults(Map<String, Object> defaults);

    void setDefaults(Configuration defaults);

    Configuration getDefaults();

    ConfigurationOptions options();
}
