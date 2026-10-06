package foton.probe;

import com.github.retrooper.packetevents.PacketEvents;
import com.github.retrooper.packetevents.event.PacketListenerAbstract;
import com.github.retrooper.packetevents.event.PacketListenerCommon;
import com.github.retrooper.packetevents.event.PacketListenerPriority;
import com.github.retrooper.packetevents.event.PacketReceiveEvent;
import com.github.retrooper.packetevents.event.PacketSendEvent;
import com.github.retrooper.packetevents.protocol.packettype.PacketType;
import com.github.retrooper.packetevents.wrapper.play.client.WrapperPlayClientPlayerFlying;
import com.github.retrooper.packetevents.wrapper.play.server.WrapperPlayServerPing;
import java.nio.file.Files;
import java.util.Map;
import java.util.TreeMap;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicInteger;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.event.EventPriority;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.player.PlayerTeleportEvent;
import org.bukkit.event.inventory.CraftItemEvent;
import org.bukkit.event.inventory.PrepareItemCraftEvent;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.ItemStack;
import org.bukkit.plugin.java.JavaPlugin;

/** A plugin that uses PacketEvents the way real ones do, and writes down what it saw.
 *
 * dev/plugin-compat-test.sh reads the report: a probe is the only way to tell
 * "the packet pipeline works" from "nothing crashed".
 */
public final class PacketProbe extends JavaPlugin implements Listener {
    private final Map<String, AtomicInteger> seen = new ConcurrentHashMap<>();
    private final Map<String, String> facts = new ConcurrentHashMap<>();
    private PacketListenerCommon listener;
    private final AtomicInteger teleports = new AtomicInteger();

    private void count(String what) { seen.computeIfAbsent(what, k -> new AtomicInteger()).incrementAndGet(); }

    @Override
    public void onEnable() {
        listener = PacketEvents.getAPI().getEventManager().registerListener(new PacketListenerAbstract(PacketListenerPriority.MONITOR) {
            @Override
            public void onPacketReceive(PacketReceiveEvent event) {
                count("in " + event.getPacketType());
                if (WrapperPlayClientPlayerFlying.isFlying(event.getPacketType())) {
                    WrapperPlayClientPlayerFlying flying = new WrapperPlayClientPlayerFlying(event);
                    if (flying.hasPositionChanged()) {
                        facts.put("last position", flying.getLocation().getX() + " " + flying.getLocation().getY() + " " + flying.getLocation().getZ());
                    }
                    if (event.getPlayer() instanceof Player player) {
                        facts.put("flying carries the player", player.getName());
                    }
                }
                if (event.getPacketType() == PacketType.Configuration.Client.PLUGIN_MESSAGE
                        || event.getPacketType() == PacketType.Configuration.Client.CLIENT_SETTINGS) {
                    facts.put("configuration user", String.valueOf(event.getUser().getName()));
                }
            }

            @Override
            public void onPacketSend(PacketSendEvent event) {
                count("out " + event.getPacketType());
                if (event.getPacketType() == PacketType.Play.Server.PING) {
                    WrapperPlayServerPing ping = new WrapperPlayServerPing(event);
                    if (ping.getId() == 424242) facts.put("tapped ping seen", "yes");
                    if (ping.getId() == 434343) facts.put("silent ping seen", "yes (wrong)");
                    event.getTasksAfterSend().add(() -> facts.put("after-send ran for ping", String.valueOf(ping.getId())));
                }
                if (event.getPacketType() == PacketType.Play.Server.JOIN_GAME) {
                    facts.put("entity id at join", String.valueOf(event.getUser().getEntityId()));
                }
            }
        });
        getServer().getPluginManager().registerEvents(this, this);
        getServer().getPluginManager().registerEvents(new CombatProbe(this, facts), this);
        getCommand("probespawn").setExecutor(new SpawnProbe(facts));
    }

    @EventHandler
    public void onJoin(PlayerJoinEvent event) {
        Player player = event.getPlayer();
        facts.put("bukkit entity id", String.valueOf(player.getEntityId()));
        var user = PacketEvents.getAPI().getPlayerManager().getUser(player);
        facts.put("user found for player", String.valueOf(user != null));
        if (user == null) return;
        facts.put("user entity id", String.valueOf(user.getEntityId()));
        facts.put("client version", user.getClientVersion().getReleaseName());
        facts.put("server version", PacketEvents.getAPI().getServerManager().getVersion().getReleaseName());
        user.sendPacket(new WrapperPlayServerPing(424242));
        user.sendPacketSilently(new WrapperPlayServerPing(434343));
    }

    /** Writes down every teleport. `/tp @s 200 ...` is refused and
     * `/tp @s 100 ...` is sent to x 50, so the report shows whether a
     * listener's answer is obeyed and not only whether it was asked. */
    @EventHandler(priority = EventPriority.HIGH)
    public void onTeleport(PlayerTeleportEvent event) {
        String cause = teleports.incrementAndGet() + " " + event.getCause().name();
        Location to = event.getTo();
        facts.put("teleport " + cause, place(event.getFrom()) + " -> " + place(to));
        if (event.getCause() == PlayerTeleportEvent.TeleportCause.COMMAND && to != null) {
            if (to.getBlockX() == 200) event.setCancelled(true);
            if (to.getBlockX() == 100) {
                Location redirected = to.clone();
                redirected.setX(50.5);
                event.setTo(redirected);
            }
        }
        Player player = event.getPlayer();
        boolean refused = event.isCancelled();
        getServer().getScheduler().runTaskLater(this,
            () -> facts.put("teleport " + cause + " then", (refused ? "refused, at " : "at ")
                + place(player.getLocation())), 2L);
    }

    /** Marks a menu as the probe's own, the way plugins recognize their menus. */
    private static final class MenuHolder implements org.bukkit.inventory.InventoryHolder {
        @Override public Inventory getInventory() { return null; }
    }
    private final MenuHolder menuHolder = new MenuHolder();
    private final AtomicInteger menuClicks = new AtomicInteger();

    /** A click in the probe's menu is recognized by its holder and refused,
     * as a plugin's menu refuses its items being taken. */
    @EventHandler
    public void onMenuClick(org.bukkit.event.inventory.InventoryClickEvent event) {
        Inventory top = event.getView().getTopInventory();
        boolean ours = event.getInventory().getHolder(false) instanceof MenuHolder;
        facts.put("menu click " + menuClicks.incrementAndGet(), "slot " + event.getRawSlot()
            + " ours " + ours + " top " + top.getType() + "/" + top.getSize()
            + " current " + item(event.getCurrentItem())
            + " click " + event.getClick() + " action " + event.getAction() + " hotbar " + event.getHotbarButton()
            + " clicked " + (event.getClickedInventory() == null ? "none" : event.getClickedInventory() == top ? "top" : "bottom"));
        // Slot 10 is the menu's button; the rest of the menu takes items.
        if (ours && event.getRawSlot() == 10) event.setCancelled(true);
    }

    /** What a plugin's menu sees as it closes: whether it is recognized,
     * and what the player left in its first slot. */
    @EventHandler
    public void onMenuClose(org.bukkit.event.inventory.InventoryCloseEvent event) {
        boolean ours = event.getInventory().getHolder(false) instanceof MenuHolder;
        facts.put("menu close", "ours " + ours + " slot 0 " + item(event.getInventory().getItem(0))
            + " slot 10 " + item(event.getInventory().getItem(10)));
    }

    private final AtomicInteger opens = new AtomicInteger();

    /** Every menu opening, as the view a plugin reads before it is shown.
     * An anvil is refused, as a plugin that disables one does. */
    @EventHandler
    public void onOpen(org.bukkit.event.inventory.InventoryOpenEvent event) {
        Inventory top = event.getView().getTopInventory();
        org.bukkit.inventory.InventoryHolder holder = top.getHolder(false);
        facts.put("open " + opens.incrementAndGet(), top.getType() + "/" + top.getSize()
            + " ours " + (holder instanceof MenuHolder)
            + " holder " + (holder == null ? "none" : holder instanceof org.bukkit.entity.Entity entity
                ? entity.getType().name() : holder.getClass().getInterfaces().length > 0
                ? holder.getClass().getInterfaces()[0].getSimpleName() : holder.getClass().getSimpleName()));
        if (top.getType() == org.bukkit.event.inventory.InventoryType.ANVIL) event.setCancelled(true);
    }

    private final AtomicInteger interactions = new AtomicInteger();

    /** Every hand use, as a plugin sees it. A right click on an emerald
     * block is refused, as a plugin guarding its own block would. */
    @EventHandler(priority = EventPriority.HIGH)
    public void onInteract(org.bukkit.event.player.PlayerInteractEvent event) {
        org.bukkit.block.Block block = event.getClickedBlock();
        String clicked = block == null ? "none" : block.getType() + "@" + block.getX() + "," + block.getY() + "," + block.getZ();
        org.bukkit.util.Vector point = event.getClickedPosition();
        facts.put("interact " + interactions.incrementAndGet(), event.getAction() + " " + event.getHand()
            + " item " + item(event.getItem()) + " block " + clicked + " face " + event.getBlockFace()
            + " point " + (point == null ? "none" : point.getX() + "," + point.getY() + "," + point.getZ())
            + " block-use " + event.useInteractedBlock() + " item-use " + event.useItemInHand());
        if (block != null && block.getType() == org.bukkit.Material.EMERALD_BLOCK) event.setCancelled(true);
    }

    private final AtomicInteger prepares = new AtomicInteger();
    private final AtomicInteger crafts = new AtomicInteger();

    /** Records what a crafting grid looks like to a plugin. A result of
     * sticks is refused, the way a plugin guarding an ingredient refuses one. */
    @EventHandler
    public void onPrepareCraft(PrepareItemCraftEvent event) {
        int n = prepares.incrementAndGet();
        ItemStack result = event.getInventory().getResult();
        facts.put("prepare craft " + n, event.getInventory().getType() + " recipe "
            + (event.getRecipe() instanceof org.bukkit.Keyed keyed ? keyed.getKey() : "none") + " matrix "
            + items(event.getInventory().getMatrix()) + " result " + item(result));
        if (result != null && result.getType() == org.bukkit.Material.STICK) {
            event.getInventory().setResult(null);
        }
    }

    @EventHandler
    public void onCraft(CraftItemEvent event) {
        facts.put("craft item " + crafts.incrementAndGet(), event.getInventory().getType() + " "
            + items(event.getInventory().getMatrix()) + " makes " + event.getRecipe().getResult().getType() + " via "
            + (event.getRecipe() instanceof org.bukkit.Keyed keyed ? keyed.getKey() : "?")
            + " current " + item(event.getCurrentItem()));
    }

    private static final org.bukkit.NamespacedKey MARKER = new org.bukkit.NamespacedKey("probe", "marker");
    private final AtomicInteger swaps = new AtomicInteger();
    private final AtomicInteger drags = new AtomicInteger();
    private final AtomicInteger stands = new AtomicInteger();

    /** A swap of hands as a plugin sees it: the items are named by where they are going.
     * Red dye going to the offhand is refused; blue dye is replaced on both hands;
     * emerald going to the main hand has the offhand replaced alone. */
    @EventHandler
    public void onSwapHands(org.bukkit.event.player.PlayerSwapHandItemsEvent event) {
        ItemStack toMain = event.getMainHandItem();
        ItemStack toOff = event.getOffHandItem();
        facts.put("swap " + swaps.incrementAndGet(), "to main " + item(toMain) + " to off " + item(toOff));
        if (toOff != null && toOff.getType() == org.bukkit.Material.RED_DYE) event.setCancelled(true);
        if (toOff != null && toOff.getType() == org.bukkit.Material.BLUE_DYE) {
            event.setMainHandItem(new ItemStack(org.bukkit.Material.GOLD_INGOT, 2));
            event.setOffHandItem(new ItemStack(org.bukkit.Material.IRON_INGOT, 3));
        }
        if (toMain != null && toMain.getType() == org.bukkit.Material.EMERALD) {
            event.setOffHandItem(new ItemStack(org.bukkit.Material.COAL, 4));
        }
    }

    private final AtomicInteger entityClicks = new AtomicInteger();

    @EventHandler
    public void onEntityClick(org.bukkit.event.player.PlayerInteractEntityEvent event) {
        facts.put("entity click " + entityClicks.incrementAndGet(), event.getRightClicked().getType() + " " + event.getHand()
            + " held " + item(event.getPlayer().getInventory().getItem(event.getHand())));
    }

    /** What a plugin sees when a player trades with an armor stand. An iron helmet is refused. */
    @EventHandler
    public void onStandManipulate(org.bukkit.event.player.PlayerArmorStandManipulateEvent event) {
        facts.put("stand " + stands.incrementAndGet(), "slot " + event.getSlot() + " hand " + event.getHand()
            + " player item " + item(event.getPlayerItem()) + " stand item " + item(event.getArmorStandItem())
            + " stand " + event.getRightClicked().getType());
        if (event.getPlayerItem().getType() == org.bukkit.Material.IRON_HELMET) event.setCancelled(true);
    }

    /** A drag, with what was on the cursor: the stack's own data must survive the crossing. */
    @EventHandler
    public void onDrag(org.bukkit.event.inventory.InventoryDragEvent event) {
        ItemStack cursor = event.getOldCursor();
        String tag = cursor == null || !cursor.hasItemMeta() ? "none"
            : cursor.getItemMeta().getPersistentDataContainer().get(MARKER, org.bukkit.persistence.PersistentDataType.STRING);
        facts.put("drag " + drags.incrementAndGet(), "type " + event.getType() + " slots " + event.getRawSlots()
            + " old cursor " + item(cursor) + " marker " + tag);
    }

    private final AtomicInteger riderMoves = new AtomicInteger();
    private final AtomicInteger riderCancels = new AtomicInteger();
    private final AtomicInteger vehicleMoves = new AtomicInteger();
    private final AtomicInteger vehicleRepels = new AtomicInteger();

    private static String pos(Location location) {
        return String.format("%.2f,%.2f,%.2f", location.getX(), location.getY(), location.getZ());
    }

    /** A player moving while riding. Reaching z 10 is refused, as a border guard refuses it. */
    @EventHandler(priority = EventPriority.HIGH)
    public void onRiderMove(org.bukkit.event.player.PlayerMoveEvent event) {
        if (!event.getPlayer().isInsideVehicle()) return;
        String move = pos(event.getFrom()) + " -> " + pos(event.getTo()) + " in " + event.getPlayer().getVehicle().getType();
        if (riderMoves.incrementAndGet() == 1) facts.put("rider move first", move);
        facts.put("rider move last", move);
        if (event.getTo().getZ() >= 10) {
            event.setCancelled(true);
            facts.put("rider move cancelled first", move);
            riderCancels.incrementAndGet();
        }
    }

    /** A boat or minecart moving. Passing x 30 is refused the way Zelda Civ refuses a border:
     * the vehicle is stopped and teleported back with its passengers. */
    @EventHandler(priority = EventPriority.HIGH)
    public void onVehicleMove(org.bukkit.event.vehicle.VehicleMoveEvent event) {
        org.bukkit.entity.Vehicle vehicle = event.getVehicle();
        String move = vehicle.getType() + " " + pos(event.getFrom()) + " -> " + pos(event.getTo())
            + " passengers " + vehicle.getPassengers().size();
        if (vehicleMoves.incrementAndGet() == 1) facts.put("vehicle move first", move);
        facts.put("vehicle move last", move);
        if (event.getTo().getX() > 30) {
            if (vehicleRepels.incrementAndGet() == 1) facts.put("vehicle repelled first", move);
            vehicle.setVelocity(new org.bukkit.util.Vector(0, 0, 0));
            facts.put("vehicle teleport answered", String.valueOf(vehicle.teleport(event.getFrom(),
                io.papermc.paper.entity.TeleportFlag.EntityState.RETAIN_PASSENGERS)));
        }
    }

    private static String item(ItemStack stack) {
        return stack == null || stack.getType().isAir() ? "-" : stack.getType() + "x" + stack.getAmount();
    }

    private static String items(ItemStack[] stacks) {
        StringBuilder out = new StringBuilder("[");
        for (int i = 0; i < stacks.length; i++) out.append(i == 0 ? "" : " ").append(item(stacks[i]));
        return out.append("]").toString();
    }

    /** `/probetp`: a plugin moves the player into another loaded world.
     * `/probecraft <label>`: writes down the top of the player's open view. */
    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        if (!(sender instanceof Player player)) return true;
        if (command.getName().equals("probegui")) {
            Inventory gui = getServer().createInventory(menuHolder, 27, net.kyori.adventure.text.Component.text("Probe"));
            gui.setItem(10, new ItemStack(org.bukkit.Material.DIAMOND, 3));
            ItemStack tagged = new ItemStack(org.bukkit.Material.EMERALD, 6);
            org.bukkit.inventory.meta.ItemMeta meta = tagged.getItemMeta();
            meta.getPersistentDataContainer().set(MARKER, org.bukkit.persistence.PersistentDataType.STRING, "kept");
            tagged.setItemMeta(meta);
            gui.setItem(12, tagged);
            facts.put("menu opened view", String.valueOf(player.openInventory(gui) != null));
            return true;
        }
        if (command.getName().equals("probehands")) {
            facts.put("hands " + (args.length > 0 ? args[0] : "?"), "main " + item(player.getInventory().getItemInMainHand())
                + " off " + item(player.getInventory().getItemInOffHand()));
            return true;
        }
        if (command.getName().equals("probestand")) {
            for (org.bukkit.entity.Entity near : player.getWorld().getEntities()) {
                if (near.getType() != org.bukkit.entity.EntityType.ARMOR_STAND) continue;
                org.bukkit.entity.ArmorStand stand = (org.bukkit.entity.ArmorStand) near;
                facts.put("stand state " + (args.length > 0 ? args[0] : "?"), "id " + stand.getEntityId() + " at " + pos(stand.getLocation()) + " helmet " + item(stand.getEquipment().getHelmet())
                    + " player main " + item(player.getInventory().getItemInMainHand()));
                return true;
            }
            facts.put("stand state " + (args.length > 0 ? args[0] : "?"), "no stand");
            return true;
        }
        if (command.getName().equals("probevehicle")) {
            org.bukkit.entity.Entity vehicle = player.getVehicle();
            facts.put("vehicle state " + (args.length > 0 ? args[0] : "?"), vehicle == null ? "not riding, player at " + pos(player.getLocation())
                : vehicle.getType() + " at " + pos(vehicle.getLocation()) + " rider " + pos(player.getLocation()));
            return true;
        }
        if (command.getName().equals("probecraft")) {
            Inventory top = player.getOpenInventory().getTopInventory();
            ItemStack[] contents = new ItemStack[top.getSize()];
            for (int i = 0; i < contents.length; i++) contents[i] = top.getItem(i);
            facts.put("view " + (args.length > 0 ? args[0] : "?"), top.getType() + " " + items(contents)
                + " cursor " + item(player.getItemOnCursor()));
            return true;
        }
        World here = player.getWorld();
        World elsewhere = null;
        for (World world : getServer().getWorlds()) {
            if (!world.getName().equals(here.getName())) { elsewhere = world; break; }
        }
        if (elsewhere == null) {
            facts.put("plugin teleport", "no second world");
            return true;
        }
        boolean moved = player.teleport(new Location(elsewhere, 0.5, 100, 0.5));
        facts.put("plugin teleport answered", String.valueOf(moved));
        getServer().getScheduler().runTaskLater(this,
            () -> facts.put("world after plugin teleport", player.getWorld().getName()), 5L);
        return true;
    }

    private static String place(Location location) {
        if (location == null) return "null";
        return (location.getWorld() == null ? "?" : location.getWorld().getName())
            + " " + location.getBlockX() + " " + location.getBlockY() + " " + location.getBlockZ();
    }

    @Override
    public void onDisable() {
        if (listener != null) PacketEvents.getAPI().getEventManager().unregisterListener(listener);
        StringBuilder report = new StringBuilder();
        report.append("fact rider moves = ").append(riderMoves.get()).append(" cancelled ").append(riderCancels.get()).append('\n');
        report.append("fact vehicle moves = ").append(vehicleMoves.get()).append(" repelled ").append(vehicleRepels.get()).append('\n');
        new TreeMap<>(facts).forEach((k, v) -> report.append("fact ").append(k).append(" = ").append(v).append('\n'));
        new TreeMap<>(seen).forEach((k, v) -> report.append("count ").append(k).append(" = ").append(v.get()).append('\n'));
        try {
            getDataFolder().mkdirs();
            Files.writeString(getDataFolder().toPath().resolve("report.txt"), report.toString());
        } catch (Exception error) {
            getLogger().severe("could not write the probe report: " + error);
        }
    }
}
