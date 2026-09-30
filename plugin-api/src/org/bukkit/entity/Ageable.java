package org.bukkit.entity;

/** Bukkit age state exposed by ageable mobs. */
public interface Ageable extends Creature {
    @Deprecated(since = "1.16.2")
    default boolean canBreed() { return foton.Native.entityCanBreed(((foton.FotonEntity) this).getUniqueId().toString()); }
    @Deprecated(since = "1.16.2")
    default void setBreed(boolean breed) { foton.Native.setEntityBreed(((foton.FotonEntity) this).getUniqueId().toString(), breed); }

    default int getAge() { return foton.Native.entityAge(((foton.FotonEntity) this).getUniqueId().toString()); }
    default void setAge(int age) { foton.Native.setEntityAge(((foton.FotonEntity) this).getUniqueId().toString(), age); }

    default boolean getAgeLock() { return foton.Native.entityAgeLock(((foton.FotonEntity) this).getUniqueId().toString()); }
    default void setAgeLock(boolean lock) { foton.Native.setEntityAgeLock(((foton.FotonEntity) this).getUniqueId().toString(), lock); }
    default boolean isAdult() {
        return !foton.Native.entityIsBaby(((foton.FotonEntity) this).getUniqueId().toString());
    }
    default void setAdult() {
        foton.Native.entitySetBaby(((foton.FotonEntity) this).getUniqueId().toString(), false);
    }
    default void setBaby() {
        foton.Native.entitySetBaby(((foton.FotonEntity) this).getUniqueId().toString(), true);
    }
}
