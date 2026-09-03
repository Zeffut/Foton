package net.minecraft.server;

import net.minecraft.core.RegistryAccess;

/** Minimal compatibility surface for plugins that probe the Paper server implementation. */
public class MinecraftServer {
    private static final MinecraftServer INSTANCE = new MinecraftServer();
    private final RegistryAccess.Frozen registryAccess = new RegistryAccess.Frozen();
    public static MinecraftServer getServer() { return INSTANCE; }
    public RegistryAccess.Frozen registryAccess() { return registryAccess; }
}
