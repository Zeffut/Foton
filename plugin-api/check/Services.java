/** Priority and lifecycle rules for Bukkit's cross-plugin service registry. */
final class Services {
    private Services() {}

    static void check() {
        org.bukkit.plugin.SimpleServicesManager services =
            new org.bukkit.plugin.SimpleServicesManager();
        org.bukkit.plugin.Plugin owner = new Owner("provider");
        Runnable low = () -> {};
        Runnable high = () -> {};

        services.register(
            Runnable.class, low, owner, org.bukkit.plugin.ServicePriority.Low);
        services.register(
            Runnable.class, high, owner, org.bukkit.plugin.ServicePriority.Highest);

        Checks.same(services.load(Runnable.class), high,
            "the highest-priority service should load");
        Checks.same(services.getRegistration(Runnable.class).getProvider(), high,
            "the winning registration should match the loaded provider");
        java.util.List<org.bukkit.plugin.RegisteredServiceProvider<Runnable>> ordered =
            java.util.List.copyOf(services.getRegistrations(Runnable.class));
        Checks.expect(ordered.size() == 2
            && ordered.get(0).getProvider() == high
            && ordered.get(1).getProvider() == low,
            "service registrations should be ordered from highest to lowest priority");

        try {
            services.getKnownServices().clear();
            throw new AssertionError("known services should be an immutable snapshot");
        } catch (UnsupportedOperationException expected) {
            // The caller cannot mutate the registry through a returned view.
        }

        services.unregister(Runnable.class, high);
        Checks.same(services.load(Runnable.class), low,
            "removing the winner should reveal the next provider");
        services.unregisterAll(owner);
        Checks.expect(!services.isProvidedFor(Runnable.class)
            && services.getKnownServices().isEmpty(),
            "unregisterAll should forget the owner's last provider and its service key");
    }

    private static final class Owner implements org.bukkit.plugin.Plugin {
        private final String name;

        Owner(String name) {
            this.name = name;
        }

        @Override public java.io.File getDataFolder() { return null; }
        @Override public org.bukkit.plugin.PluginDescriptionFile getDescription() { return null; }
        @Override public org.bukkit.Server getServer() { return null; }
        @Override public java.util.logging.Logger getLogger() { return null; }
        @Override public String getName() { return name; }
        @Override public boolean isEnabled() { return true; }
        @Override public void onEnable() {}
        @Override public void onDisable() {}
    }
}
