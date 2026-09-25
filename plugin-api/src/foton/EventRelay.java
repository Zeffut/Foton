package foton;

import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.entity.Player;
import org.bukkit.util.Vector;

/** Paper's events that Foton learned to carry after the first set.
 *
 * Every method has the same shape, so the Rust side writes the crossing once
 * (`foton-plugin/src/relay.rs`): the facts arrive as strings, the event is
 * built and dispatched through {@link EventBridge#dispatch}, and the outcome
 * goes back as one string whose fields are separated by {@link #FIELD}, with
 * booleans written `1` or `0`.
 */
public final class EventRelay {
    /** Separates the fields of an answer. */
    static final String FIELD = "\u001f";

    private EventRelay() {}

    static String bit(boolean value) {
        return value ? "1" : "0";
    }

    static String answer(Object... fields) {
        StringBuilder out = new StringBuilder();
        for (int index = 0; index < fields.length; index++) {
            if (index > 0) out.append(FIELD);
            Object field = fields[index];
            out.append(field instanceof Boolean flag ? bit(flag) : String.valueOf(field));
        }
        return out.toString();
    }

    static Player player(String uuid) {
        UUID parsed = Native.parse(uuid);
        return parsed == null ? null : new FotonPlayer(parsed);
    }

    static boolean flag(String value) {
        return "1".equals(value);
    }

    /** `x y z [yaw pitch]` in a world, as the relay writes a location. */
    static Location location(World world, String encoded) {
        String[] parts = encoded.split(" ");
        double x = Double.parseDouble(parts[0]);
        double y = Double.parseDouble(parts[1]);
        double z = Double.parseDouble(parts[2]);
        if (parts.length < 5) return new Location(world, x, y, z);
        return new Location(world, x, y, z, Float.parseFloat(parts[3]), Float.parseFloat(parts[4]));
    }

    static Vector vector(String encoded) {
        String[] parts = encoded.split(" ");
        return new Vector(Double.parseDouble(parts[0]), Double.parseDouble(parts[1]),
            Double.parseDouble(parts[2]));
    }

    static String encode(Vector vector) {
        return vector.getX() + " " + vector.getY() + " " + vector.getZ();
    }

    /** Answers `allowed, logWarning`. */
    public static String fireFailMove(String uuid, String world, String reason, String from,
            String to, String logWarning) {
        World in = new FotonWorld(world);
        io.papermc.paper.event.player.PlayerFailMoveEvent event =
            new io.papermc.paper.event.player.PlayerFailMoveEvent(player(uuid),
                io.papermc.paper.event.player.PlayerFailMoveEvent.FailReason.valueOf(reason),
                false, flag(logWarning), location(in, from), location(in, to));
        EventBridge.dispatch(event);
        return answer(event.isAllowed(), event.getLogWarning());
    }

    /** Answers `cancelled`. */
    public static String fireToggleFlight(String uuid, String flying) {
        org.bukkit.event.player.PlayerToggleFlightEvent event =
            new org.bukkit.event.player.PlayerToggleFlightEvent(player(uuid), flag(flying));
        EventBridge.dispatch(event);
        return answer(event.isCancelled());
    }

    /** Answers `cancelled, velocity`. */
    public static String fireVelocity(String uuid, String velocity) {
        org.bukkit.event.player.PlayerVelocityEvent event =
            new org.bukkit.event.player.PlayerVelocityEvent(player(uuid), vector(velocity));
        EventBridge.dispatch(event);
        return answer(event.isCancelled(), encode(event.getVelocity()));
    }

    /** Answers nothing: the change has already happened. */
    public static String fireArmorChange(String uuid, String slot, String oldItem, String newItem) {
        com.destroystokyo.paper.event.player.PlayerArmorChangeEvent event =
            new com.destroystokyo.paper.event.player.PlayerArmorChangeEvent(player(uuid),
                com.destroystokyo.paper.event.player.PlayerArmorChangeEvent.SlotType.valueOf(slot),
                FotonInventory.decode(oldItem), FotonInventory.decode(newItem));
        EventBridge.dispatch(event);
        return "";
    }

    /** Answers `cancelled, item, hasReplacement, replacement`. */
    public static String fireItemConsume(String uuid, String hand, String item) {
        org.bukkit.event.player.PlayerItemConsumeEvent event =
            new org.bukkit.event.player.PlayerItemConsumeEvent(player(uuid),
                FotonInventory.decode(item), org.bukkit.inventory.EquipmentSlot.valueOf(hand));
        EventBridge.dispatch(event);
        org.bukkit.inventory.ItemStack replacement = event.getReplacement();
        return answer(event.isCancelled(), FotonInventory.encode(event.getItem()),
            replacement != null, replacement == null ? "" : FotonInventory.encode(replacement));
    }

    static FotonBlock block(String world, String position) {
        String[] parts = position.split(" ");
        return new FotonBlock(new FotonWorld(world), Integer.parseInt(parts[0]),
            Integer.parseInt(parts[1]), Integer.parseInt(parts[2]));
    }

    static org.bukkit.inventory.EquipmentSlot hand(String hand) {
        return org.bukkit.inventory.EquipmentSlot.valueOf(hand);
    }

    static java.util.List<org.bukkit.entity.Entity> entities(String uuids) {
        java.util.List<org.bukkit.entity.Entity> list = new java.util.ArrayList<>();
        if (uuids == null || uuids.isEmpty()) return list;
        for (String uuid : uuids.split(",")) {
            org.bukkit.entity.Entity entity = FotonEntity.handle(Native.parse(uuid));
            if (entity != null) list.add(entity);
        }
        return list;
    }

    /** Answers `cancelled`. */
    public static String fireEntityPlace(String entity, String player, String world,
            String block, String face, String hand) {
        org.bukkit.event.entity.EntityPlaceEvent event = new org.bukkit.event.entity.EntityPlaceEvent(
            FotonEntity.handle(Native.parse(entity)), player(player), block(world, block),
            org.bukkit.block.BlockFace.valueOf(face), hand(hand));
        EventBridge.dispatch(event);
        return answer(event.isCancelled());
    }

    /** Answers `cancelled`. */
    public static String fireDismount(String entity, String vehicle, String cancellable) {
        org.bukkit.event.entity.EntityDismountEvent event = new org.bukkit.event.entity.EntityDismountEvent(
            FotonEntity.handle(Native.parse(entity)), FotonEntity.handle(Native.parse(vehicle)),
            flag(cancellable));
        EventBridge.dispatch(event);
        return answer(event.isCancelled());
    }

    /** Answers nothing. */
    public static String fireEntitiesLoad(String world, String chunk, String uuids) {
        String[] at = chunk.split(" ");
        EventBridge.dispatch(new org.bukkit.event.world.EntitiesLoadEvent(
            new FotonChunk(new FotonWorld(world), Integer.parseInt(at[0]), Integer.parseInt(at[1])),
            entities(uuids)));
        return "";
    }

    /** Answers nothing. */
    public static String fireEntitiesUnload(String world, String chunk, String uuids) {
        String[] at = chunk.split(" ");
        EventBridge.dispatch(new org.bukkit.event.world.EntitiesUnloadEvent(
            new FotonChunk(new FotonWorld(world), Integer.parseInt(at[0]), Integer.parseInt(at[1])),
            entities(uuids)));
        return "";
    }

    /** Answers `cancelled, damage`. */
    public static String fireEnvironmentDamage(String entity, String cause, String damage) {
        org.bukkit.event.entity.EntityDamageEvent event = new org.bukkit.event.entity.EntityDamageEvent(
            FotonEntity.handle(Native.parse(entity)),
            org.bukkit.event.entity.EntityDamageEvent.DamageCause.valueOf(cause),
            Double.parseDouble(damage));
        EventBridge.dispatch(event);
        if (event.getEntity() != null) EventBridge.setLastDamageCause(event.getEntity().getUniqueId(), event);
        return answer(event.isCancelled(), event.getDamage());
    }

    /** Separates the stacks of a list. */
    static final String ITEM = "\u001e";

    static java.util.List<org.bukkit.inventory.ItemStack> stacks(String encoded) {
        java.util.List<org.bukkit.inventory.ItemStack> list = new java.util.ArrayList<>();
        if (encoded == null || encoded.isEmpty()) return list;
        for (String one : encoded.split(ITEM, -1)) {
            org.bukkit.inventory.ItemStack stack = FotonInventory.decode(one);
            if (stack != null) list.add(stack);
        }
        return list;
    }

    static String encodeStacks(java.util.List<org.bukkit.inventory.ItemStack> stacks) {
        StringBuilder out = new StringBuilder();
        for (org.bukkit.inventory.ItemStack stack : stacks) {
            if (stack == null) continue;
            if (out.length() > 0) out.append(ITEM);
            out.append(FotonInventory.encode(stack));
        }
        return out.toString();
    }

    /** Answers `cancelled, dropItems, expToDrop`. */
    public static String fireBlockBreak(String uuid, String world, String block, String exp) {
        org.bukkit.event.block.BlockBreakEvent event =
            new org.bukkit.event.block.BlockBreakEvent(block(world, block), player(uuid));
        event.setExpToDrop(Integer.parseInt(exp));
        EventBridge.dispatch(event);
        return answer(event.isCancelled(), event.isDropItems(), event.getExpToDrop());
    }

    /** Answers `cancelled, kept item uuids`. */
    public static String fireBlockDropItem(String uuid, String world, String block, String state,
            String items) {
        FotonBlock at = block(world, block);
        java.util.List<org.bukkit.entity.Item> dropped = new java.util.ArrayList<>();
        for (org.bukkit.entity.Entity entity : entities(items)) {
            if (entity instanceof org.bukkit.entity.Item item) dropped.add(item);
        }
        org.bukkit.event.block.BlockDropItemEvent event = new org.bukkit.event.block.BlockDropItemEvent(
            at, new FotonBlockState(at, FotonBlock.dataOf(state)), player(uuid), dropped);
        EventBridge.dispatch(event);
        StringBuilder kept = new StringBuilder();
        for (org.bukkit.entity.Item item : event.getItems()) {
            if (item == null) continue;
            if (kept.length() > 0) kept.append(',');
            kept.append(item.getUniqueId());
        }
        return answer(event.isCancelled(), kept);
    }

    /** A cooking recipe as the relay describes one:
     * `key, experience, cookingTime, result, inputs`, typed by the block that
     * cooks it. Null for an empty or unreadable description. */
    static org.bukkit.inventory.CookingRecipe<?> cookingRecipe(org.bukkit.Material block, String description) {
        if (description == null || description.isEmpty()) return null;
        String[] parts = description.split(FIELD, -1);
        if (parts.length < 5) return null;
        org.bukkit.NamespacedKey key = org.bukkit.NamespacedKey.fromString(parts[0]);
        if (key == null) return null;
        float experience = Float.parseFloat(parts[1]);
        int time = Integer.parseInt(parts[2]);
        org.bukkit.inventory.ItemStack result = FotonInventory.decode(parts[3]);
        java.util.List<org.bukkit.Material> inputs = new java.util.ArrayList<>();
        for (String input : parts[4].split(" ")) {
            org.bukkit.Material material = input.isEmpty() ? null : org.bukkit.Material.matchMaterial(input);
            if (material != null) inputs.add(material);
        }
        org.bukkit.inventory.RecipeChoice choice = new org.bukkit.inventory.RecipeChoice.MaterialChoice(inputs);
        return switch (block) {
            case BLAST_FURNACE -> new org.bukkit.inventory.BlastingRecipe(key, result, choice, experience, time);
            case SMOKER -> new org.bukkit.inventory.SmokingRecipe(key, result, choice, experience, time);
            default -> new org.bukkit.inventory.FurnaceRecipe(key, result, choice, experience, time);
        };
    }

    /** Stacks slot by slot: an empty entry is an empty slot, kept in place. */
    static java.util.List<org.bukkit.inventory.ItemStack> slots(String encoded) {
        java.util.List<org.bukkit.inventory.ItemStack> list = new java.util.ArrayList<>();
        for (String one : encoded.split(ITEM, -1)) {
            org.bukkit.inventory.ItemStack stack = FotonInventory.decode(one);
            list.add(stack == null ? new org.bukkit.inventory.ItemStack(org.bukkit.Material.AIR) : stack);
        }
        return list;
    }

    /** Answers `cancelled, burnTime, burning, consumeFuel`. */
    public static String fireFurnaceBurn(String world, String block, String fuel, String burnTime) {
        org.bukkit.event.inventory.FurnaceBurnEvent event = new org.bukkit.event.inventory.FurnaceBurnEvent(
            block(world, block), FotonInventory.decode(fuel), Integer.parseInt(burnTime));
        EventBridge.dispatch(event);
        return answer(event.isCancelled(), event.getBurnTime(), event.isBurning(), event.willConsumeFuel());
    }

    /** Answers `totalCookTime`. */
    public static String fireFurnaceStartSmelt(String world, String block, String source, String recipe,
            String total) {
        FotonBlock at = block(world, block);
        org.bukkit.event.inventory.FurnaceStartSmeltEvent event = new org.bukkit.event.inventory.FurnaceStartSmeltEvent(
            at, FotonInventory.decode(source), cookingRecipe(at.getType(), recipe), Integer.parseInt(total));
        EventBridge.dispatch(event);
        return answer(event.getTotalCookTime());
    }

    /** Answers `cancelled, result`. */
    public static String fireFurnaceSmelt(String world, String block, String source, String result,
            String recipe) {
        FotonBlock at = block(world, block);
        org.bukkit.event.inventory.FurnaceSmeltEvent event = new org.bukkit.event.inventory.FurnaceSmeltEvent(
            at, FotonInventory.decode(source), FotonInventory.decode(result), cookingRecipe(at.getType(), recipe));
        EventBridge.dispatch(event);
        return answer(event.isCancelled(), FotonInventory.encode(event.getResult()));
    }

    /** Answers `cancelled, results`, the results slot by slot. */
    public static String fireBrew(String world, String block, String results, String fuel) {
        FotonBlock at = block(world, block);
        org.bukkit.event.inventory.BrewEvent event = new org.bukkit.event.inventory.BrewEvent(
            at, new FotonBrewerInventory(new FotonBlockState(at, at.getBlockData())), slots(results),
            Integer.parseInt(fuel));
        EventBridge.dispatch(event);
        StringBuilder out = new StringBuilder();
        java.util.List<org.bukkit.inventory.ItemStack> brewed = event.getResults();
        for (int slot = 0; slot < 3; slot++) {
            if (slot > 0) out.append(ITEM);
            if (slot < brewed.size()) out.append(FotonInventory.encode(brewed.get(slot)));
        }
        return answer(event.isCancelled(), out);
    }

    /** Answers `cancelled, offers`, each offer `key level cost` or empty. */
    public static String firePrepareEnchant(String uuid, String world, String table, String item,
            String offers, String bonus, String cancelled) {
        Player player = player(uuid);
        String[] rows = offers.split(ITEM, -1);
        org.bukkit.enchantments.EnchantmentOffer[] parsed = new org.bukkit.enchantments.EnchantmentOffer[3];
        for (int slot = 0; slot < parsed.length && slot < rows.length; slot++) {
            String[] parts = rows[slot].split(" ");
            if (parts.length != 3) continue;
            org.bukkit.NamespacedKey key = org.bukkit.NamespacedKey.fromString(parts[0]);
            org.bukkit.enchantments.Enchantment enchantment =
                key == null ? null : org.bukkit.enchantments.Enchantment.getByKey(key);
            if (enchantment == null) continue;
            parsed[slot] = new org.bukkit.enchantments.EnchantmentOffer(enchantment,
                Integer.parseInt(parts[1]), Integer.parseInt(parts[2]));
        }
        org.bukkit.event.enchantment.PrepareItemEnchantEvent event =
            new org.bukkit.event.enchantment.PrepareItemEnchantEvent(player, player.getOpenInventory(),
                block(world, table), FotonInventory.decode(item), parsed, Integer.parseInt(bonus));
        event.setCancelled(flag(cancelled));
        EventBridge.dispatch(event);
        StringBuilder out = new StringBuilder();
        org.bukkit.enchantments.EnchantmentOffer[] left = event.getOffers();
        for (int slot = 0; slot < 3; slot++) {
            if (slot > 0) out.append(ITEM);
            org.bukkit.enchantments.EnchantmentOffer offer = slot < left.length ? left[slot] : null;
            if (offer == null || offer.getEnchantment() == null) continue;
            out.append(offer.getEnchantment().getKey()).append(' ').append(offer.getEnchantmentLevel())
                .append(' ').append(offer.getCost());
        }
        return answer(event.isCancelled(), out);
    }

    /** Answers the result. */
    public static String firePrepareSmithing(String uuid, String inputs, String result) {
        Player player = player(uuid);
        java.util.List<org.bukkit.inventory.ItemStack> slots = slots(inputs);
        org.bukkit.inventory.ItemStack offered = FotonInventory.decode(result);
        slots.add(offered == null ? new org.bukkit.inventory.ItemStack(org.bukkit.Material.AIR) : offered);
        FotonSmithingInventory inventory = new FotonSmithingInventory(uuid,
            slots.toArray(new org.bukkit.inventory.ItemStack[0]));
        org.bukkit.event.inventory.PrepareSmithingEvent event = new org.bukkit.event.inventory.PrepareSmithingEvent(
            new FotonInventoryView((FotonPlayer) player, inventory), offered);
        EventBridge.dispatch(event);
        return FotonInventory.encode(event.getResult());
    }

    /** An offer as the relay writes one: `result, costA, costB, uses,
     * maxUses, rewardExp, xp, priceMultiplier, demand`. */
    static org.bukkit.inventory.MerchantRecipe merchantRecipe(String encoded) {
        String[] parts = encoded.split("\u001c", -1);
        if (parts.length != 9) return null;
        org.bukkit.inventory.MerchantRecipe recipe = new org.bukkit.inventory.MerchantRecipe(
            FotonInventory.decode(parts[0]), Integer.parseInt(parts[3]), Integer.parseInt(parts[4]),
            flag(parts[5]), Integer.parseInt(parts[6]), Float.parseFloat(parts[7]), Integer.parseInt(parts[8]));
        org.bukkit.inventory.ItemStack first = FotonInventory.decode(parts[1]);
        if (first != null) recipe.addIngredient(first);
        org.bukkit.inventory.ItemStack second = FotonInventory.decode(parts[2]);
        if (second != null) recipe.addIngredient(second);
        return recipe;
    }

    /** Answers `cancelled`. A trade with a mob is a {@code PlayerTradeEvent}. */
    public static String firePurchase(String uuid, String trader, String offer, String offers) {
        Player player = player(uuid);
        org.bukkit.inventory.MerchantRecipe trade = merchantRecipe(offer);
        org.bukkit.entity.Entity mob = trader.isEmpty() ? null : FotonEntity.handle(Native.parse(trader));
        io.papermc.paper.event.player.PlayerPurchaseEvent event;
        if (mob instanceof org.bukkit.entity.AbstractVillager villager) {
            event = new io.papermc.paper.event.player.PlayerTradeEvent(player, villager, trade, true, true);
        } else {
            java.util.List<org.bukkit.inventory.MerchantRecipe> recipes = new java.util.ArrayList<>();
            for (String one : offers.isEmpty() ? new String[0] : offers.split(ITEM, -1)) {
                org.bukkit.inventory.MerchantRecipe recipe = merchantRecipe(one);
                if (recipe != null) recipes.add(recipe);
            }
            event = new io.papermc.paper.event.player.PlayerPurchaseEvent(player,
                new FotonMerchantScreen(recipes), trade, false, true);
        }
        EventBridge.dispatch(event);
        return answer(event.isCancelled());
    }

    /** The event a click in a menu raises: a crafting table's or a smithing
     * table's result slot raises the event for crafting or forging, as Paper
     * does, and every other click an {@code InventoryClickEvent}. */
    static org.bukkit.event.inventory.InventoryClickEvent clickEvent(Player player,
            org.bukkit.inventory.ItemStack current, org.bukkit.inventory.ItemStack cursor,
            org.bukkit.event.inventory.ClickType click, int rawSlot) {
        if (player == null || current == null || current.getType().isAir()) {
            return new org.bukkit.event.inventory.InventoryClickEvent(player, current, cursor, click, rawSlot);
        }
        String type = Native.openMenuType(player.getUniqueId().toString());
        if ("minecraft:crafting".equals(type) && rawSlot == 0) {
            org.bukkit.inventory.Inventory top = player.getOpenInventory().getTopInventory();
            if (top instanceof org.bukkit.inventory.CraftingInventory crafting) {
                StringBuilder grid = new StringBuilder();
                org.bukkit.inventory.ItemStack[] matrix = crafting.getMatrix();
                for (int index = 0; index < matrix.length; index++) {
                    if (index > 0) grid.append(ITEM);
                    grid.append(FotonInventory.encode(matrix[index]));
                }
                String key = Native.craftingRecipe(grid.toString(), 3);
                org.bukkit.NamespacedKey recipe = key == null ? null : org.bukkit.NamespacedKey.fromString(key);
                if (recipe != null) {
                    return new org.bukkit.event.inventory.CraftItemEvent(new FotonCraftingRecipe(recipe, current),
                        player, current, cursor, click, rawSlot);
                }
            }
        }
        if ("minecraft:smithing".equals(type) && rawSlot == 3) {
            return new org.bukkit.event.inventory.SmithItemEvent(player, current, cursor, click, rawSlot);
        }
        return new org.bukkit.event.inventory.InventoryClickEvent(player, current, cursor, click, rawSlot);
    }

    /** Answers `motd, maxPlayers`, the MOTD as JSON text. */
    public static String fireServerListPing(String address, String motd, String online, String max) {
        java.net.InetAddress from;
        try {
            from = java.net.InetAddress.getByName(address);
        } catch (java.net.UnknownHostException unreadable) {
            from = null;
        }
        org.bukkit.event.server.ServerListPingEvent event = new org.bukkit.event.server.ServerListPingEvent(
            from, FotonText.component(motd), Integer.parseInt(online), Integer.parseInt(max));
        EventBridge.dispatch(event);
        return answer(FotonText.json(event.motd()), event.getMaxPlayers());
    }

    /** Answers `cancelled, world, x y z yaw pitch`. */
    public static String fireTeleport(String uuid, String fromWorld, String from, String toWorld,
            String to, String cause) {
        Location origin = location(new FotonWorld(fromWorld), from);
        Location destination = location(new FotonWorld(toWorld), to);
        org.bukkit.event.player.PlayerTeleportEvent event = new org.bukkit.event.player.PlayerTeleportEvent(
            player(uuid), origin, destination, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause.valueOf(cause));
        EventBridge.dispatch(event);
        Location target = event.getTo() == null || event.getTo().getWorld() == null ? destination : event.getTo();
        return answer(event.isCancelled(), target.getWorld().getName(), target.getX() + " " + target.getY()
            + " " + target.getZ() + " " + target.getYaw() + " " + target.getPitch());
    }

    /** Answers `cancelled, stacks`. */
    public static String fireHarvest(String uuid, String world, String block, String hand,
            String items) {
        org.bukkit.event.player.PlayerHarvestBlockEvent event =
            new org.bukkit.event.player.PlayerHarvestBlockEvent(player(uuid), block(world, block),
                hand(hand), stacks(items));
        EventBridge.dispatch(event);
        return answer(event.isCancelled(), encodeStacks(event.getItemsHarvested()));
    }
}
