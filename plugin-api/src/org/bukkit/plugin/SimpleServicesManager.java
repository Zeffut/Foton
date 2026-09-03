package org.bukkit.plugin;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Set;
import org.bukkit.Bukkit;
import org.bukkit.event.server.ServiceRegisterEvent;
import org.bukkit.event.server.ServiceUnregisterEvent;

/** Bukkit's priority-ordered, thread-safe service registry. */
public class SimpleServicesManager implements ServicesManager {
    private final Map<Class<?>, List<RegisteredServiceProvider<?>>> providers =
        new HashMap<>();

    @Override
    public <T> void register(
            Class<T> service, T provider, Plugin plugin, ServicePriority priority) {
        RegisteredServiceProvider<T> registration =
            new RegisteredServiceProvider<>(service, provider, priority, plugin);
        synchronized (providers) {
            List<RegisteredServiceProvider<?>> registered =
                providers.computeIfAbsent(service, ignored -> new ArrayList<>());
            int position = Collections.binarySearch(registered, registration);
            registered.add(position < 0 ? -(position + 1) : position, registration);
        }
        fireRegistered(registration);
    }

    @Override
    public void unregisterAll(Plugin plugin) {
        List<RegisteredServiceProvider<?>> removed = new ArrayList<>();
        synchronized (providers) {
            removeMatching(removed, registration -> registration.getPlugin().equals(plugin));
        }
        fireRemoved(removed);
    }

    @Override
    public void unregister(Class<?> service, Object provider) {
        List<RegisteredServiceProvider<?>> removed = new ArrayList<>();
        synchronized (providers) {
            List<RegisteredServiceProvider<?>> registered = providers.get(service);
            if (registered != null) {
                registered.removeIf(registration -> {
                    if (registration.getProvider() != provider) {
                        return false;
                    }
                    removed.add(registration);
                    return true;
                });
                if (registered.isEmpty()) {
                    providers.remove(service);
                }
            }
        }
        fireRemoved(removed);
    }

    @Override
    public void unregister(Object provider) {
        List<RegisteredServiceProvider<?>> removed = new ArrayList<>();
        synchronized (providers) {
            removeMatching(removed, registration -> registration.getProvider().equals(provider));
        }
        fireRemoved(removed);
    }

    @Override
    public <T> T load(Class<T> service) {
        RegisteredServiceProvider<T> registration = getRegistration(service);
        return registration == null ? null : registration.getProvider();
    }

    @Override
    public <T> RegisteredServiceProvider<T> getRegistration(Class<T> service) {
        synchronized (providers) {
            List<RegisteredServiceProvider<?>> registered = providers.get(service);
            if (registered == null || registered.isEmpty()) {
                return null;
            }
            return cast(registered.get(0));
        }
    }

    @Override
    public List<RegisteredServiceProvider<?>> getRegistrations(Plugin plugin) {
        List<RegisteredServiceProvider<?>> answer = new ArrayList<>();
        synchronized (providers) {
            for (List<RegisteredServiceProvider<?>> registered : providers.values()) {
                for (RegisteredServiceProvider<?> registration : registered) {
                    if (registration.getPlugin().equals(plugin)) {
                        answer.add(registration);
                    }
                }
            }
        }
        return List.copyOf(answer);
    }

    @Override
    public <T> Collection<RegisteredServiceProvider<T>> getRegistrations(Class<T> service) {
        synchronized (providers) {
            List<RegisteredServiceProvider<?>> registered = providers.get(service);
            if (registered == null) {
                return List.of();
            }
            List<RegisteredServiceProvider<T>> answer = new ArrayList<>(registered.size());
            for (RegisteredServiceProvider<?> registration : registered) {
                answer.add(cast(registration));
            }
            return List.copyOf(answer);
        }
    }

    @Override
    public Collection<Class<?>> getKnownServices() {
        synchronized (providers) {
            return Set.copyOf(new HashSet<>(providers.keySet()));
        }
    }

    @Override
    public <T> boolean isProvidedFor(Class<T> service) {
        synchronized (providers) {
            return providers.containsKey(service);
        }
    }

    private void removeMatching(
            List<RegisteredServiceProvider<?>> removed,
            java.util.function.Predicate<RegisteredServiceProvider<?>> matches) {
        Iterator<Map.Entry<Class<?>, List<RegisteredServiceProvider<?>>>> entries =
            providers.entrySet().iterator();
        while (entries.hasNext()) {
            List<RegisteredServiceProvider<?>> registered = entries.next().getValue();
            registered.removeIf(registration -> {
                if (!matches.test(registration)) {
                    return false;
                }
                removed.add(registration);
                return true;
            });
            if (registered.isEmpty()) {
                entries.remove();
            }
        }
    }

    private static void fireRemoved(List<RegisteredServiceProvider<?>> removed) {
        if (Bukkit.getServer() == null) {
            return;
        }
        for (RegisteredServiceProvider<?> registration : removed) {
            Bukkit.getPluginManager().callEvent(new ServiceUnregisterEvent(registration));
        }
    }

    /** Events run after the registry lock is released, so a handler may query it safely. */
    private static void fireRegistered(RegisteredServiceProvider<?> registration) {
        if (Bukkit.getServer() != null) {
            Bukkit.getPluginManager().callEvent(new ServiceRegisterEvent(registration));
        }
    }

    @SuppressWarnings("unchecked")
    private static <T> RegisteredServiceProvider<T> cast(
            RegisteredServiceProvider<?> registration) {
        return (RegisteredServiceProvider<T>) registration;
    }
}
