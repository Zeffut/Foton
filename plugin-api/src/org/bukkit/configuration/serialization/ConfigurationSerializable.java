package org.bukkit.configuration.serialization;
import java.util.Map;
/** A value that can be represented in Bukkit configuration data. */
public interface ConfigurationSerializable { Map<String,Object> serialize(); }
