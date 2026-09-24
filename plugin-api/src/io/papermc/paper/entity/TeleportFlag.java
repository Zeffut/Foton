package io.papermc.paper.entity;

/** How a teleport treats what the entity carries, rides and moves with. */
public sealed interface TeleportFlag permits TeleportFlag.EntityState, TeleportFlag.Relative {
    /** Velocity components kept relative to the entity's new orientation. */
    enum Relative implements TeleportFlag {
        VELOCITY_X,
        VELOCITY_Y,
        VELOCITY_Z,
        VELOCITY_ROTATION;

        @Deprecated public static final Relative X = VELOCITY_X;
        @Deprecated public static final Relative Y = VELOCITY_Y;
        @Deprecated public static final Relative Z = VELOCITY_Z;
        @Deprecated public static final Relative YAW = VELOCITY_ROTATION;
        @Deprecated public static final Relative PITCH = VELOCITY_ROTATION;
    }

    /** What the teleported entity keeps. */
    @Deprecated
    enum EntityState implements TeleportFlag {
        /** Its passengers come with it; without this a vehicle with riders does not move. */
        RETAIN_PASSENGERS,
        /** It stays in its vehicle; without this it is dismounted first. */
        RETAIN_VEHICLE,
        RETAIN_OPEN_INVENTORY
    }
}
