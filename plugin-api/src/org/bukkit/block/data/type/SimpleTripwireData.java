package org.bukkit.block.data.type;

import org.bukkit.block.data.SimpleBlockData;

/** Vanilla tripwire data backed by Foton's state string. */
public final class SimpleTripwireData extends SimpleBlockData implements Tripwire {
    public SimpleTripwireData(String text) { super(text); }
    @Override public boolean isAttached() { return property("attached"); }
    @Override public void setPowered(boolean value) { property("powered", value); }
}
