package org.bukkit;

import java.util.Collection;

/** A goat horn's sound, one entry of the vanilla {@code instrument} registry.
 *
 * <p>An abstract class, as in Paper. Its constants are generated from the data
 * pack by {@code dev/gen-registry-values.py}, and so are the duration, range
 * and sound each one reports.</p>
 *
 * <p>Initialising this class reads the registry, and the registry creates
 * subclasses of it, whose creation initialises this class. That cycle is safe
 * only because the registry lives in {@link Registry}'s fields, which never
 * wait on this class, and because {@link foton.FotonKeyedRegistry} holds no
 * lock while it creates a value and keeps whichever copy was stored first.</p>
 */
public abstract class MusicInstrument implements Keyed, net.kyori.adventure.translation.Translatable {
    // @@CONSTANTS@@

    @Deprecated
    public static MusicInstrument getByKey(NamespacedKey key) {
        return Registry.INSTRUMENT.get(key);
    }

    @Deprecated
    public static Collection<MusicInstrument> values() {
        return Registry.INSTRUMENT.stream().toList();
    }

    /** How long the horn plays, in seconds. */
    public abstract float getDuration();

    /** How far away, in blocks, the horn can be heard. */
    public abstract float getRange();

    public abstract net.kyori.adventure.text.Component description();

    public abstract Sound getSound();

    @Override
    @Deprecated
    public abstract NamespacedKey getKey();

    @Override
    @Deprecated
    public abstract String translationKey();
}
