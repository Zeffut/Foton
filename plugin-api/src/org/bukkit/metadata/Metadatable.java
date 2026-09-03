package org.bukkit.metadata;

import org.bukkit.plugin.Plugin;
import java.util.List;

public interface Metadatable {
    default void setMetadata(String metadataKey, MetadataValue newMetadataValue) {
        foton.FotonMetadataBridge.set(this, metadataKey, newMetadataValue);
    }
    default List<MetadataValue> getMetadata(String metadataKey) {
        return foton.FotonMetadataBridge.get(this, metadataKey);
    }
    default boolean hasMetadata(String metadataKey) { return !getMetadata(metadataKey).isEmpty(); }
    default void removeMetadata(String metadataKey, Plugin owningPlugin) {
        foton.FotonMetadataBridge.remove(this, metadataKey, owningPlugin);
    }
}
