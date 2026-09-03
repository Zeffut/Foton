package io.papermc.paper.datapack;

public interface Datapack {
    enum Compatibility { TOO_OLD, TOO_NEW, COMPATIBLE }
    String getName();
    Compatibility getCompatibility();
    boolean isEnabled();
}
