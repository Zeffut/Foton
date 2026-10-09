package foton.entity;

/** The class a plugin finds when it looks for CraftBukkit's player.
 *
 * Plugins that reach past the Bukkit API name this class by convention: the
 * package of {@code Bukkit.getServer().getClass()} plus
 * {@code .entity.CraftPlayer}. Simple Voice Chat requires it, with
 * {@code addChannel} and {@code removeChannel}, before it starts at all, and
 * ViaVersion looks for it at login. Every player Foton hands out is one.
 *
 * <p>Only what CraftBukkit itself offers here, and only what Foton can honor,
 * is declared: there is no Minecraft server object behind it to hand out.
 *
 * <p>It extends {@link foton.FotonLivingEntity}, so a player answers every
 * entity and living-entity query through the same natives as any other mob.
 */
public abstract class CraftPlayer extends foton.FotonLivingEntity
        implements org.bukkit.entity.Player, net.kyori.adventure.audience.Audience {
    protected CraftPlayer(java.util.UUID id) { super(id); }

    // Declared by both Player and Audience; every player implements its own.
    @Override public abstract void resetTitle();
    @Override public abstract void showTitle(net.kyori.adventure.title.Title title);
    @Override public abstract void sendMessage(net.kyori.adventure.text.Component message);
    @Override public abstract void sendResourcePacks(net.kyori.adventure.resource.ResourcePackRequest request);
    @Override public abstract void removeResourcePacks(java.util.UUID id, java.util.UUID... others);
    @Override public abstract void clearResourcePacks();

    // Player and Audience both carry these as defaults; Player's are the ones
    // that reach the client, so they are chosen here, where Player is direct.
    @Override
    public void sendSignChange(org.bukkit.Location location, String[] lines, org.bukkit.DyeColor color) {
        org.bukkit.entity.Player.super.sendSignChange(location, lines, color);
    }

    @Override
    public void sendResourcePacks(net.kyori.adventure.resource.ResourcePackRequestLike request) {
        org.bukkit.entity.Player.super.sendResourcePacks(request);
    }

    @Override
    public void sendResourcePacks(net.kyori.adventure.resource.ResourcePackInfoLike first,
            net.kyori.adventure.resource.ResourcePackInfoLike... others) {
        org.bukkit.entity.Player.super.sendResourcePacks(first, others);
    }

    @Override
    public void removeResourcePacks(Iterable<java.util.UUID> ids) {
        org.bukkit.entity.Player.super.removeResourcePacks(ids);
    }

    @Override
    public void removeResourcePacks(net.kyori.adventure.resource.ResourcePackRequestLike request) {
        org.bukkit.entity.Player.super.removeResourcePacks(request);
    }

    @Override
    public void removeResourcePacks(net.kyori.adventure.resource.ResourcePackRequest request) {
        org.bukkit.entity.Player.super.removeResourcePacks(request);
    }

    @Override
    public void removeResourcePacks(net.kyori.adventure.resource.ResourcePackInfoLike first,
            net.kyori.adventure.resource.ResourcePackInfoLike... others) {
        org.bukkit.entity.Player.super.removeResourcePacks(first, others);
    }

    @Override
    public void sendActionBar(net.kyori.adventure.text.Component message) {
        org.bukkit.entity.Player.super.sendActionBar(message);
    }

    @Override
    public void clearTitle() {
        org.bukkit.entity.Player.super.clearTitle();
    }

    @Override
    public void sendPlayerListHeaderAndFooter(net.kyori.adventure.text.Component header,
            net.kyori.adventure.text.Component footer) {
        org.bukkit.entity.Player.super.sendPlayerListHeaderAndFooter(header, footer);
    }

    @Override
    public void sendPlayerListHeader(net.kyori.adventure.text.Component header) {
        org.bukkit.entity.Player.super.sendPlayerListHeader(header);
    }

    /** Registers the player as listening on a plugin channel, as the client's
     * {@code minecraft:register} does; true if it was not already. */
    public abstract boolean addChannel(String channel);

    /** The reverse of {@link #addChannel}. */
    public abstract boolean removeChannel(String channel);
}
