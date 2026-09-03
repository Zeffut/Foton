package org.bukkit.configuration.serialization;

import java.lang.reflect.Method;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/** Bukkit serialization registry and reflective deserializer. */
public final class ConfigurationSerialization {
    private static final Map<String, Class<? extends ConfigurationSerializable>> REGISTRY = new ConcurrentHashMap<>();
    private ConfigurationSerialization() {}
    public static void registerClass(Class<? extends ConfigurationSerializable> clazz) {
        if (clazz == null) throw new IllegalArgumentException("clazz");
        REGISTRY.put(clazz.getName(), clazz); REGISTRY.put(clazz.getSimpleName(), clazz);
    }
    public static void unregisterClass(Class<? extends ConfigurationSerializable> clazz) {
        if (clazz != null) { REGISTRY.remove(clazz.getName(), clazz); REGISTRY.remove(clazz.getSimpleName(), clazz); }
    }
    public static Class<? extends ConfigurationSerializable> getClassByAlias(String alias) { return alias == null ? null : REGISTRY.get(alias); }
    public static ConfigurationSerializable deserializeObject(Map<String, Object> args, Class<? extends ConfigurationSerializable> clazz) {
        if (clazz == null) throw new IllegalArgumentException("clazz");
        try {
            Method method = clazz.getDeclaredMethod("deserialize", Map.class);
            return (ConfigurationSerializable) method.invoke(null, args);
        } catch (NoSuchMethodException missing) {
            try {
                Method method = clazz.getDeclaredMethod("valueOf", Map.class);
                return (ConfigurationSerializable) method.invoke(null, args);
            } catch (ReflectiveOperationException unavailable) {
                throw new IllegalArgumentException("No Bukkit deserializer on " + clazz.getName(), unavailable);
            }
        } catch (ReflectiveOperationException error) {
            throw new IllegalArgumentException("Could not deserialize " + clazz.getName(), error);
        }
    }
}
