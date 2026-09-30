package org.bukkit.inventory.meta;

import org.bukkit.MusicInstrument;

/** A goat horn's meta: its instrument is the item's instrument component. */
public final class SimpleMusicInstrumentMeta extends SimpleItemMeta implements MusicInstrumentMeta {
    @Override public void setInstrument(MusicInstrument instrument) { setInstrumentComponent(instrument); }
    @Override public MusicInstrument getInstrument() { return getInstrumentComponent(); }
    @Override public SimpleMusicInstrumentMeta clone() { return (SimpleMusicInstrumentMeta) super.clone(); }
}
