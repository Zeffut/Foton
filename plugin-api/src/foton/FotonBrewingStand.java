package foton;

import org.bukkit.Location;
import org.bukkit.block.Block;
import org.bukkit.block.BrewingStand;
import org.bukkit.block.data.BlockData;
import org.bukkit.inventory.ItemStack;

/** Snapshot-backed brewing stand holder used by BrewEvent. */
public final class FotonBrewingStand extends FotonTileState implements BrewingStand {
    private final FotonBrewerInventory inventory;
    private int brewingTime;
    private int recipeBrewTime;
    private int fuelLevel;
    private String customName;
    private String lock = "";
    private ItemStack lockItem;

    FotonBrewingStand(Block block, BlockData data, ItemStack[] contents,
            int brewingTime, int recipeBrewTime, int fuelLevel) {
        super(block, data);
        this.brewingTime = brewingTime;
        this.recipeBrewTime = recipeBrewTime;
        this.fuelLevel = fuelLevel;
        this.inventory = new FotonBrewerInventory(this, contents);
    }

    static FotonBrewingStand eventSnapshot(FotonBlock block, ItemStack[] contents, int fuelLevel) {
        return new FotonBrewingStand(block,
            new org.bukkit.block.data.SimpleBlockData("minecraft:brewing_stand"),
            contents, 0, 400, fuelLevel);
    }

    static FotonBrewingStand placedSnapshot(FotonBlock block, BlockData data) {
        ItemStack[] contents = new ItemStack[5];
        String world = block.getWorld().getName();
        for (int slot = 0; slot < contents.length; slot++) {
            contents[slot] = FotonInventory.decodeTransfer(Native.hopperInventorySlot(
                world, block.getX(), block.getY(), block.getZ(), slot));
        }
        int brewingTime = 0;
        int fuelLevel = 0;
        String state = Native.brewingStandState(
            world, block.getX(), block.getY(), block.getZ());
        if (state != null) {
            String[] fields = state.split("\u001f", -1);
            try {
                if (fields.length > 0) brewingTime = Integer.parseInt(fields[0]);
                if (fields.length > 1) fuelLevel = Integer.parseInt(fields[1]);
            } catch (NumberFormatException ignored) { }
        }
        return new FotonBrewingStand(
            block, data, contents, brewingTime, 400, fuelLevel);
    }

    @Override public Location getLocation() { return getBlock().getLocation(); }
    @Override public int getBrewingTime() { return brewingTime; }
    @Override public void setBrewingTime(int value) { brewingTime = value; }
    @Override public void setRecipeBrewTime(int value) {
        if (value < 1) throw new IllegalArgumentException("recipe brew time must be positive");
        recipeBrewTime = value;
    }
    @Override public int getRecipeBrewTime() { return recipeBrewTime; }
    @Override public int getFuelLevel() { return fuelLevel; }
    @Override public void setFuelLevel(int value) { fuelLevel = value; }
    @Override public FotonBrewerInventory getInventory() { return inventory; }
    @Override public FotonBrewerInventory getSnapshotInventory() {
        return new FotonBrewerInventory(this, inventory.getContents());
    }
    @Override public boolean isLocked() {
        return !lock.isEmpty() || (lockItem != null && !lockItem.getType().isAir());
    }
    @Override public String getLock() { return lock; }
    @Override public void setLock(String value) {
        if (value == null) throw new IllegalArgumentException("lock cannot be null");
        lock = value;
        lockItem = null;
    }
    @Override public void setLockItem(ItemStack value) {
        if (value == null) throw new IllegalArgumentException("lock item cannot be null");
        lockItem = value.clone();
        lock = "";
    }
    @Override public net.kyori.adventure.text.Component customName() {
        return customName == null ? null : net.kyori.adventure.text.Component.text(customName);
    }
    @Override public void customName(net.kyori.adventure.text.Component value) {
        customName = value == null ? null
            : net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer
                .plainText().serialize(value);
    }
    @Override public String getCustomName() { return customName; }
    @Override public void setCustomName(String value) { customName = value; }
}
