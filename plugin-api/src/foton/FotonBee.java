package foton;

import java.util.UUID;

/** Live Bukkit view of a Steel bee. */
public final class FotonBee extends FotonLivingEntity implements org.bukkit.entity.Bee {
    public FotonBee(UUID id) { super(id); }
    @Override public int getAnger() { return Native.beeAnger(getUniqueId().toString()); }
    @Override public void setAnger(int anger) { Native.setBeeAnger(getUniqueId().toString(), anger); }
    @Override public boolean hasNectar() { return Native.beeHasNectar(getUniqueId().toString()); }
    @Override public void setHasNectar(boolean value) { Native.setBeeHasNectar(getUniqueId().toString(), value); }
    @Override public boolean hasStung() { return Native.beeHasStung(getUniqueId().toString()); }
    @Override public void setHasStung(boolean value) { Native.setBeeHasStung(getUniqueId().toString(), value); }
}
