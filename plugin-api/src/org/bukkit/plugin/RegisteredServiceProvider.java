package org.bukkit.plugin;

/** One provider registered for one service. */
public class RegisteredServiceProvider<T>
        implements Comparable<RegisteredServiceProvider<?>> {
    private final Class<T> service;
    private final Plugin plugin;
    private final T provider;
    private final ServicePriority priority;

    public RegisteredServiceProvider(
            Class<T> service, T provider, ServicePriority priority, Plugin plugin) {
        this.service = service;
        this.plugin = plugin;
        this.provider = provider;
        this.priority = priority;
    }

    public Class<T> getService() {
        return service;
    }

    public Plugin getPlugin() {
        return plugin;
    }

    public T getProvider() {
        return provider;
    }

    public ServicePriority getPriority() {
        return priority;
    }

    /** Higher priorities sort first, which makes the first registration the winner. */
    @Override
    public int compareTo(RegisteredServiceProvider<?> other) {
        return Integer.compare(other.priority.ordinal(), priority.ordinal());
    }
}
