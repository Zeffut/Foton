package org.bukkit.configuration.file;

import org.bukkit.configuration.MemoryConfiguration;
import org.bukkit.configuration.MemoryConfigurationOptions;

/** The options a configuration in a file carries.
 *
 * Every setter narrows its return type. That is not decoration: a plugin
 * writes `config.options().copyDefaults(true).header("...")`, and the compiler
 * resolved `copyDefaults` against this class, so a base-class return type
 * makes the next call in the chain fail to link.
 */
public class FileConfigurationOptions extends MemoryConfigurationOptions {
    private String header;
    private boolean copyHeader = true;

    protected FileConfigurationOptions(MemoryConfiguration configuration) {
        super(configuration);
    }

    @Override
    public FileConfiguration configuration() {
        return (FileConfiguration) super.configuration();
    }

    @Override
    public FileConfigurationOptions copyDefaults(boolean value) {
        super.copyDefaults(value);
        return this;
    }

    @Override
    public FileConfigurationOptions pathSeparator(char value) {
        super.pathSeparator(value);
        return this;
    }

    public String header() {
        return header;
    }

    public FileConfigurationOptions setHeader(java.util.List<String> lines) {
        return header(lines == null ? null : String.join("\n", lines));
    }

    public FileConfigurationOptions header(String value) {
        this.header = value;
        return this;
    }

    public boolean copyHeader() {
        return copyHeader;
    }

    public FileConfigurationOptions copyHeader(boolean value) {
        this.copyHeader = value;
        return this;
    }
}
