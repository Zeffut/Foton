package org.bukkit;

/** Which fluids a ray trace stops at. */
public enum FluidCollisionMode {
    /** Rays pass through every fluid. */
    NEVER,
    /** Rays stop at source blocks only. */
    SOURCE_ONLY,
    /** Rays stop at any fluid. */
    ALWAYS
}
