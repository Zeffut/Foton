package foton;

import java.util.UUID;

/** Any vanilla equine: horse, donkey, mule, llama, camel, skeleton and zombie horse.
 *
 * <p>Paper's {@code AbstractHorse} is a tameable vehicle with an inventory, and
 * a plugin holding one by that interface expects every equine to answer it. */
public class FotonAbstractHorse extends FotonTameableEntity implements org.bukkit.entity.AbstractHorse {
    public FotonAbstractHorse(UUID id) { super(id); }

    @Override public org.bukkit.inventory.AbstractHorseInventory getInventory() {
        return new FotonHorseInventory(getUniqueId().toString());
    }
    @Override public int getDomestication() { return Native.horseTemper(getUniqueId().toString()); }
    @Override public void setDomestication(int value) { Native.setHorseTemper(getUniqueId().toString(), value); }
    @Override public int getMaxDomestication() { return Native.horseMaxTemper(getUniqueId().toString()); }
}
