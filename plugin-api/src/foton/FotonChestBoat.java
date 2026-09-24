package foton;

import java.util.UUID;

/** Live Bukkit view of a chest boat or chest raft. */
public final class FotonChestBoat extends FotonBoat implements org.bukkit.entity.ChestBoat {
    public FotonChestBoat(UUID id) { super(id); }

    /** The chest, with any loot table it still carries rolled first. */
    @Override public org.bukkit.inventory.Inventory getInventory() { return new FotonCarriedInventory(this); }
}
