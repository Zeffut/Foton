package foton.probe;

import com.destroystokyo.paper.profile.ProfileProperty;
import java.net.URI;
import java.util.Map;
import org.bukkit.Bukkit;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.attribute.Attribute;
import org.bukkit.attribute.AttributeModifier;
import org.bukkit.command.Command;
import org.bukkit.command.CommandExecutor;
import org.bukkit.command.CommandSender;
import org.bukkit.entity.Player;
import org.bukkit.inventory.EquipmentSlotGroup;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.inventory.meta.SkullMeta;

/** `/probeitems`: gives the player an elytra with attribute modifiers (built the
 * way Zelda Civ's Piaf flight builds one) and two player heads, then reads each
 * back out of the inventory and writes down what the meta says. */
final class ItemProbe implements CommandExecutor {
    private final Map<String, String> facts;

    ItemProbe(Map<String, String> facts) {
        this.facts = facts;
    }

    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        if (!(sender instanceof Player player)) return true;
        try {
            giveElytra(player);
        } catch (Throwable error) {
            facts.put("items elytra", "threw " + error);
        }
        try {
            giveHeads(player);
        } catch (Throwable error) {
            facts.put("items heads", "threw " + error);
        }
        return true;
    }

    private void giveElytra(Player player) {
        ItemStack elytra = new ItemStack(Material.ELYTRA);
        ItemMeta meta = elytra.getItemMeta();
        add(meta, Attribute.ARMOR, "probe_armor", 8, AttributeModifier.Operation.ADD_NUMBER, EquipmentSlotGroup.CHEST);
        add(meta, Attribute.ARMOR_TOUGHNESS, "probe_scalar", 0.5, AttributeModifier.Operation.ADD_SCALAR, EquipmentSlotGroup.CHEST);
        add(meta, Attribute.KNOCKBACK_RESISTANCE, "probe_total", 0.1, AttributeModifier.Operation.MULTIPLY_SCALAR_1, EquipmentSlotGroup.ANY);
        elytra.setItemMeta(meta);
        // Zelda copies another item's modifiers into a new meta, entry by entry.
        ItemStack copy = new ItemStack(Material.ELYTRA);
        ItemMeta copyMeta = copy.getItemMeta();
        for (var entry : elytra.getItemMeta().getAttributeModifiers().entries())
            copyMeta.addAttributeModifier(entry.getKey(), entry.getValue());
        copy.setItemMeta(copyMeta);
        facts.put("items elytra hasItemMeta", String.valueOf(elytra.hasItemMeta()));
        player.getInventory().addItem(elytra);
        for (ItemStack stack : player.getInventory().getContents()) {
            if (stack == null || stack.getType() != Material.ELYTRA) continue;
            ItemMeta read = stack.getItemMeta();
            StringBuilder out = new StringBuilder("has=" + read.hasAttributeModifiers());
            for (var entry : read.getAttributeModifiers().entries()) {
                AttributeModifier modifier = entry.getValue();
                out.append(" [").append(entry.getKey().getKey()).append(' ').append(modifier.getKey())
                    .append(' ').append(modifier.getAmount()).append(' ').append(modifier.getOperation())
                    .append(' ').append(modifier.getSlotGroup()).append(']');
            }
            facts.put("items elytra read back", out.toString());
            facts.put("items elytra copy equal", String.valueOf(copy.isSimilar(stack)));
            return;
        }
        facts.put("items elytra read back", "not found");
    }

    private static void add(ItemMeta meta, Attribute attribute, String key, double amount,
            AttributeModifier.Operation operation, EquipmentSlotGroup slot) {
        meta.addAttributeModifier(attribute, new AttributeModifier(new NamespacedKey("probe", key), amount, operation, slot));
    }

    private void giveHeads(Player player) throws Exception {
        ItemStack own = new ItemStack(Material.PLAYER_HEAD);
        SkullMeta ownMeta = (SkullMeta) own.getItemMeta();
        ownMeta.setOwningPlayer(player);
        own.setItemMeta(ownMeta);
        player.getInventory().addItem(own);

        var profile = (com.destroystokyo.paper.profile.PlayerProfile) Bukkit.createPlayerProfile(java.util.UUID.nameUUIDFromBytes("probe".getBytes()), "ProbeHead");
        profile.getTextures().setSkin(URI.create("https://textures.minecraft.net/texture/abcdef").toURL());
        ItemStack custom = new ItemStack(Material.PLAYER_HEAD);
        SkullMeta customMeta = (SkullMeta) custom.getItemMeta();
        customMeta.setPlayerProfile(profile);
        custom.setItemMeta(customMeta);
        player.getInventory().addItem(custom);

        int found = 0;
        for (ItemStack stack : player.getInventory().getContents()) {
            if (stack == null || stack.getType() != Material.PLAYER_HEAD) continue;
            var read = ((SkullMeta) stack.getItemMeta()).getPlayerProfile();
            StringBuilder out = new StringBuilder();
            out.append("name=").append(read == null ? null : read.getName());
            out.append(" id=").append(read == null ? null : read.getUniqueId());
            if (read != null) {
                for (ProfileProperty property : read.getProperties())
                    out.append(" property=").append(property.getName()).append(" signed=").append(property.isSigned());
                out.append(" skin=").append(read.getTextures().getSkin());
            }
            facts.put("items head " + found++, out.toString());
        }
        facts.put("items head count", String.valueOf(found));
    }
}
