package foton;

import java.util.List;
import org.bukkit.entity.Entity;
import org.bukkit.metadata.MetadataValue;
import org.bukkit.plugin.Plugin;

public final class FotonMetadataBridge {
    private FotonMetadataBridge() { }
    public static void set(Object owner, String key, MetadataValue value) {
        if (owner instanceof Entity entity) FotonMetadata.set(entity.getUniqueId(), key, value);
        else FotonMetadata.set(owner, key, value);
    }
    public static List<MetadataValue> get(Object owner, String key) {
        if (owner instanceof Entity entity) return FotonMetadata.get(entity.getUniqueId(), key);
        return FotonMetadata.get(owner, key);
    }
    public static void remove(Object owner, String key, Plugin plugin) {
        if (owner instanceof Entity entity) FotonMetadata.remove(entity.getUniqueId(), key, plugin);
        else FotonMetadata.remove(owner, key, plugin);
    }
}
