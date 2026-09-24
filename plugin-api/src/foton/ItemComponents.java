package foton;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HexFormat;
import java.util.List;
import java.util.Locale;
import net.kyori.adventure.text.Component;
import org.bukkit.DyeColor;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;
import org.bukkit.block.banner.Pattern;
import org.bukkit.block.banner.PatternType;
import org.bukkit.inventory.meta.BookMeta;
import org.bukkit.inventory.meta.SimpleBookMeta;
import org.bukkit.inventory.meta.SimpleItemMeta;
import org.bukkit.inventory.meta.trim.ArmorTrim;
import org.bukkit.inventory.meta.trim.TrimMaterial;
import org.bukkit.inventory.meta.trim.TrimPattern;

/** The vanilla components of an item that cross JNI beside the ones
 * {@link FotonInventory} has always carried.
 *
 * <p>One field per component, {@code name=value}, in the slot string's
 * {@code \u001d}-separated list; foton-plugin's {@code describe_slot} and
 * {@code parse_slot} write and read the same fields, so a plugin's banner,
 * trim, cooldown or custom data is the item the client sees and the world
 * saves.</p>
 */
final class ItemComponents {
    private ItemComponents() { }

    private static final String SEP = "\u001d";

    static String encode(SimpleItemMeta meta, Material type) {
        StringBuilder out = new StringBuilder();
        if (meta.displayName() != null) field(out, "namejsonhex", hex(ComponentJson.json(meta.displayName())));
        if (meta.hasLore()) for (Component line : meta.lore()) field(out, "lorejsonhex", hex(ComponentJson.json(line)));
        for (String hidden : meta.getHiddenComponents()) field(out, "hide", hidden);
        if (meta.hasEnchantmentGlintOverride()) field(out, "glint", String.valueOf(meta.getEnchantmentGlintOverride()));
        if (meta.hasMaxStackSize()) field(out, "maxstack", String.valueOf(meta.getMaxStackSize()));
        if (meta.hasUseCooldown()) {
            field(out, "cooldown", String.valueOf(meta.getUseCooldown().getCooldownSeconds()));
            NamespacedKey group = meta.getUseCooldown().getCooldownGroup();
            if (group != null) field(out, "cooldowngroup", group.toString());
        }
        List<Pattern> patterns = meta.getBannerPatternsComponent();
        if (patterns != null) {
            for (Pattern pattern : patterns) {
                if (pattern.getPattern() == null || pattern.getColor() == null) continue;
                field(out, "pattern", pattern.getPattern().getKey() + "," + dye(pattern.getColor()));
            }
        }
        if (meta.getBaseColorComponent() != null) field(out, "basecolor", dye(meta.getBaseColorComponent()));
        ArmorTrim trim = meta.getTrimComponent();
        if (trim != null) field(out, "trim", trim.getMaterial().getKey() + "," + trim.getPattern().getKey());
        if (meta.getInstrumentComponent() != null) field(out, "instrument", meta.getInstrumentComponent().getKey().toString());
        java.util.Map<String, Object> custom = meta.getCustomData();
        if (!custom.isEmpty()) field(out, "customhex", hex(Snbt.write(custom)));
        if (meta instanceof SimpleBookMeta book) book(out, book, type);
        return out.toString();
    }

    private static void book(StringBuilder out, SimpleBookMeta book, Material type) {
        if (type == Material.WRITTEN_BOOK) {
            // A written book always has its cover, even an empty one: vanilla's
            // component has no absent title or author.
            field(out, "booktitlehex", hex(book.hasTitle() ? book.getTitle() : ""));
            field(out, "bookauthorhex", hex(book.hasAuthor() ? book.getAuthor() : ""));
            BookMeta.Generation generation = book.getGeneration();
            field(out, "bookgen", String.valueOf(generation == null ? 0 : generation.ordinal()));
            for (Component page : book.pages()) field(out, "bookpagehex", hex(ComponentJson.json(page)));
        } else if (type == Material.WRITABLE_BOOK) {
            for (String page : book.getPages()) field(out, "bookpagehex", hex(page));
        }
    }

    static void decode(SimpleItemMeta meta, Material type, List<String> fields) {
        List<String> hidden = new ArrayList<>();
        List<Pattern> patterns = new ArrayList<>();
        List<Component> pages = new ArrayList<>();
        List<Component> lore = new ArrayList<>();
        Float cooldown = null;
        NamespacedKey cooldownGroup = null;
        for (String field : fields) {
            int equals = field.indexOf('=');
            if (equals < 0) continue;
            String name = field.substring(0, equals);
            String value = field.substring(equals + 1);
            try {
                switch (name) {
                    case "namejsonhex" -> meta.displayName(ComponentJson.parse(unhex(value)));
                    case "lorejsonhex" -> lore.add(ComponentJson.parse(unhex(value)));
                    case "hide" -> hidden.add(value);
                    case "glint" -> meta.setEnchantmentGlintOverride(Boolean.parseBoolean(value));
                    case "maxstack" -> meta.setMaxStackSize(Integer.parseInt(value));
                    case "cooldown" -> cooldown = Float.parseFloat(value);
                    case "cooldowngroup" -> cooldownGroup = NamespacedKey.fromString(value);
                    case "pattern" -> {
                        String[] parts = value.split(",", 2);
                        PatternType pattern = Registry.BANNER_PATTERN.get(NamespacedKey.fromString(parts[0]));
                        DyeColor color = dye(parts[1]);
                        if (pattern != null && color != null) patterns.add(new Pattern(color, pattern));
                    }
                    case "basecolor" -> meta.setBaseColorComponent(dye(value));
                    case "trim" -> {
                        String[] parts = value.split(",", 2);
                        TrimMaterial material = Registry.TRIM_MATERIAL.get(NamespacedKey.fromString(parts[0]));
                        TrimPattern pattern = Registry.TRIM_PATTERN.get(NamespacedKey.fromString(parts[1]));
                        if (material != null && pattern != null) meta.setTrimComponent(new ArmorTrim(material, pattern));
                    }
                    case "instrument" -> meta.setInstrumentComponent(Registry.INSTRUMENT.get(NamespacedKey.fromString(value)));
                    case "customhex" -> meta.setCustomData(Snbt.parseCompound(unhex(value)));
                    case "booktitlehex" -> {
                        if (meta instanceof SimpleBookMeta book) book.setTitle(unhex(value));
                    }
                    case "bookauthorhex" -> {
                        if (meta instanceof SimpleBookMeta book) book.setAuthor(unhex(value));
                    }
                    case "bookgen" -> {
                        int generation = Integer.parseInt(value);
                        if (meta instanceof SimpleBookMeta book && generation >= 0 && generation < BookMeta.Generation.values().length) {
                            book.setGeneration(BookMeta.Generation.values()[generation]);
                        }
                    }
                    case "bookpagehex" -> pages.add(type == Material.WRITTEN_BOOK
                        ? ComponentJson.parse(unhex(value)) : Component.text(unhex(value)));
                    default -> { }
                }
            } catch (RuntimeException unreadable) {
                // One malformed field loses that component, not the item.
            }
        }
        if (!lore.isEmpty()) meta.lore(lore);
        if (!hidden.isEmpty()) meta.setHiddenComponents(hidden);
        if (!patterns.isEmpty()) meta.setBannerPatternsComponent(patterns);
        if (cooldown != null && cooldown > 0) {
            meta.setUseCooldown(new org.bukkit.inventory.meta.components.SimpleUseCooldownComponent(cooldown, cooldownGroup));
        }
        if (!pages.isEmpty() && meta instanceof SimpleBookMeta book) book.pages(pages);
    }

    private static void field(StringBuilder out, String name, String value) {
        out.append(SEP).append(name).append('=').append(value);
    }

    private static String dye(DyeColor color) {
        return color.name().toLowerCase(Locale.ROOT);
    }

    private static DyeColor dye(String name) {
        try {
            return DyeColor.valueOf(name.toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException unknown) {
            return null;
        }
    }

    static String hex(String value) {
        return HexFormat.of().formatHex(value.getBytes(StandardCharsets.UTF_8));
    }

    static String unhex(String value) {
        return new String(HexFormat.of().parseHex(value), StandardCharsets.UTF_8);
    }
}
