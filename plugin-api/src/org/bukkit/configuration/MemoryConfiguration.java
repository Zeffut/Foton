package org.bukkit.configuration;

import java.util.Map;

/** A configuration held only in memory. */
public class MemoryConfiguration extends MemorySection implements Configuration {
    protected Configuration defaults;
    protected MemoryConfigurationOptions options;

    public MemoryConfiguration() {
        super();
    }

    @Override
    public void addDefaults(Map<String, Object> values) {
        for (Map.Entry<String, Object> entry : values.entrySet()) {
            defaultsSection().set(entry.getKey(), entry.getValue());
        }
    }

    private Configuration defaultsSection() {
        if (defaults == null) {
            defaults = new MemoryConfiguration();
        }
        return defaults;
    }

    @Override
    public void setDefaults(Configuration defaults) {
        this.defaults = defaults;
    }

    @Override
    public Configuration getDefaults() {
        return defaults;
    }

    @Override
    public MemoryConfigurationOptions options() {
        if (options == null) {
            options = new MemoryConfigurationOptions(this);
        }
        return options;
    }
}
