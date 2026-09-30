import io.papermc.paper.datacomponent.DataComponentTypes;
import io.papermc.paper.datacomponent.item.BannerPatternLayers;
import java.util.List;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import org.bukkit.DyeColor;
import org.bukkit.Material;
import org.bukkit.MusicInstrument;
import org.bukkit.NamespacedKey;
import org.bukkit.block.banner.Pattern;
import org.bukkit.block.banner.PatternType;
import org.bukkit.inventory.ItemFlag;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.ArmorMeta;
import org.bukkit.inventory.meta.BookMeta;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.inventory.meta.MusicInstrumentMeta;
import org.bukkit.inventory.meta.trim.ArmorTrim;
import org.bukkit.inventory.meta.trim.TrimMaterial;
import org.bukkit.inventory.meta.trim.TrimPattern;
import org.bukkit.persistence.PersistentDataType;

/** Components a plugin sets on an item survive the slot string to the server and back. */
final class ItemComponentsCheck {
    private ItemComponentsCheck() {}

    private static ItemStack roundTrip(ItemStack item) {
        return foton.FotonInventory.decode(foton.FotonInventory.encode(item));
    }

    static void check() {
        NamespacedKey owner = NamespacedKey.fromString("zelda:owner");
        NamespacedKey marks = NamespacedKey.fromString("zelda:marks");

        ItemStack armor = new ItemStack(Material.DIAMOND_CHESTPLATE);
        Checks.expect(armor.getItemMeta() instanceof ArmorMeta, "trimmable armor has an ArmorMeta");
        ArmorMeta armorMeta = (ArmorMeta) armor.getItemMeta();
        armorMeta.setTrim(new ArmorTrim(TrimMaterial.GOLD, TrimPattern.WILD));
        armorMeta.addItemFlags(ItemFlag.HIDE_ADDITIONAL_TOOLTIP, ItemFlag.HIDE_ATTRIBUTES);
        armorMeta.setEnchantmentGlintOverride(true);
        armorMeta.getPersistentDataContainer().set(owner, PersistentDataType.STRING, "link");
        armorMeta.getPersistentDataContainer().set(marks, PersistentDataType.INTEGER_ARRAY, new int[] {1, 2, 3});
        armor.setItemMeta(armorMeta);
        Checks.expect(armor.hasData(DataComponentTypes.TRIM), "a trim set through the meta is the TRIM component");

        ItemStack armorRead = roundTrip(armor);
        ArmorMeta armorBack = (ArmorMeta) armorRead.getItemMeta();
        Checks.same(armorBack.getTrim(), new ArmorTrim(TrimMaterial.GOLD, TrimPattern.WILD), "the trim survives");
        Checks.expect(armorBack.hasItemFlag(ItemFlag.HIDE_ADDITIONAL_TOOLTIP)
            && armorBack.hasItemFlag(ItemFlag.HIDE_ATTRIBUTES)
            && !armorBack.hasItemFlag(ItemFlag.HIDE_ENCHANTS), "flags survive as hidden components");
        Checks.expect(armorBack.getEnchantmentGlintOverride(), "the glint override survives");
        Checks.same(armorRead.getPersistentDataContainer().get(owner, PersistentDataType.STRING), "link",
            "persistent data survives, readable without a meta");
        Checks.expect(java.util.Arrays.equals(
                armorRead.getPersistentDataContainer().get(marks, PersistentDataType.INTEGER_ARRAY), new int[] {1, 2, 3}),
            "an int array survives");
        Checks.same(armorRead.getData(DataComponentTypes.TRIM).armorTrim().getPattern(), TrimPattern.WILD,
            "the TRIM component reads back");
        boolean mismatch = false;
        try { armorRead.getPersistentDataContainer().get(owner, PersistentDataType.INTEGER); }
        catch (IllegalArgumentException expected) { mismatch = true; }
        Checks.expect(mismatch, "reading a string as an integer is refused, as in Paper");

        ItemStack shield = new ItemStack(Material.SHIELD);
        List<Pattern> layers = List.of(new Pattern(DyeColor.RED, PatternType.CROSS),
            new Pattern(DyeColor.BLUE, PatternType.BORDER));
        shield.setData(DataComponentTypes.BASE_COLOR, DyeColor.WHITE);
        shield.setData(DataComponentTypes.BANNER_PATTERNS, BannerPatternLayers.bannerPatternLayers(layers));
        ItemStack shieldRead = roundTrip(shield);
        Checks.expect(shieldRead.getData(DataComponentTypes.BASE_COLOR) == DyeColor.WHITE, "a shield keeps its base colour");
        Checks.same(shieldRead.getData(DataComponentTypes.BANNER_PATTERNS).patterns(), layers, "and its layers, in order");
        shieldRead.unsetData(DataComponentTypes.BANNER_PATTERNS);
        shieldRead.unsetData(DataComponentTypes.BASE_COLOR);
        Checks.expect(!roundTrip(shieldRead).hasData(DataComponentTypes.BASE_COLOR), "an unset component stays unset");

        ItemStack horn = new ItemStack(Material.GOAT_HORN);
        MusicInstrumentMeta hornMeta = (MusicInstrumentMeta) horn.getItemMeta();
        hornMeta.setInstrument(MusicInstrument.DREAM_GOAT_HORN);
        org.bukkit.inventory.meta.components.UseCooldownComponent cooldown = hornMeta.getUseCooldown();
        cooldown.setCooldownSeconds(12.5f);
        cooldown.setCooldownGroup(NamespacedKey.fromString("zelda:whistle"));
        hornMeta.setUseCooldown(cooldown);
        hornMeta.setMaxStackSize(1);
        horn.setItemMeta(hornMeta);
        MusicInstrumentMeta hornBack = (MusicInstrumentMeta) roundTrip(horn).getItemMeta();
        Checks.expect(hornBack.getInstrument() == MusicInstrument.DREAM_GOAT_HORN, "the instrument survives");
        Checks.same(hornBack.getUseCooldown().getCooldownGroup(), NamespacedKey.fromString("zelda:whistle"),
            "the cooldown group survives");
        Checks.same(hornBack.getUseCooldown().getCooldownSeconds(), 12.5f, "the cooldown survives");
        Checks.same(roundTrip(horn).getMaxStackSize(), 1, "the stack size component wins over the type's");

        ItemStack book = new ItemStack(Material.WRITTEN_BOOK);
        BookMeta bookMeta = (BookMeta) book.getItemMeta();
        bookMeta.title(Component.text("Livre", NamedTextColor.GOLD));
        bookMeta.author(Component.text("Hyrule"));
        bookMeta.pages(List.of(Component.text("one", NamedTextColor.RED), Component.text("two")));
        book.setItemMeta(bookMeta);
        BookMeta bookBack = (BookMeta) roundTrip(book).getItemMeta();
        Checks.same(bookBack.getTitle(), "§6Livre", "a component title is stored as section-sign text");
        Checks.same(bookBack.getAuthor(), "Hyrule", "the author survives");
        Checks.same(bookBack.pages().get(0), Component.text("one", NamedTextColor.RED), "a styled page survives");
        Checks.same(bookBack.getPage(2), "two", "a plain page reads as its text");

        // Team prefixes and book pages cross as JSON; a plugin comparing what
        // it set with what it reads back (to skip a redundant packet) needs
        // the trip to be exact.
        Component styled = Component.text("[Hylien] ", NamedTextColor.GOLD)
            .decoration(net.kyori.adventure.text.format.TextDecoration.BOLD, true)
            .shadowColor(net.kyori.adventure.text.format.ShadowColor.none())
            .append(Component.translatable("race.hylian", Component.text("x")));
        Checks.same(foton.ComponentJson.parse(foton.ComponentJson.json(styled)), styled,
            "a component survives its JSON form");

        ItemStack gem = new ItemStack(Material.EMERALD);
        ItemMeta gemMeta = gem.getItemMeta();
        Component gemName = Component.text("Rubis", NamedTextColor.RED)
            .decoration(net.kyori.adventure.text.format.TextDecoration.ITALIC, false);
        gemMeta.displayName(gemName);
        gemMeta.lore(List.of(Component.text("précieux", NamedTextColor.GRAY)));
        gem.setItemMeta(gemMeta);
        ItemMeta gemBack = roundTrip(gem).getItemMeta();
        Checks.same(gemBack.displayName(), gemName, "a coloured name survives the slot string");
        Checks.same(gemBack.getDisplayName(), "§cRubis", "and reads as section-sign text");
        Checks.same(gemBack.lore(), List.of(Component.text("précieux", NamedTextColor.GRAY)), "coloured lore survives");

        ItemMeta plain = new ItemStack(Material.STONE).getItemMeta();
        plain.addItemFlags(ItemFlag.HIDE_ENCHANTS);
        plain.removeItemFlags(ItemFlag.HIDE_ENCHANTS);
        Checks.expect(plain.getItemFlags().isEmpty(), "a removed flag is gone");
    }
}
