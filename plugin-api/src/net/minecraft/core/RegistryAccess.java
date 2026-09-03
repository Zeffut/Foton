package net.minecraft.core;

/** Minimal registry access marker used by Paper command integrations. */
public class RegistryAccess {
    public static class Frozen extends RegistryAccess implements HolderLookup.Provider {}
}
