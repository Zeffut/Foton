import java.util.List;
import org.bukkit.Material;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.inventory.meta.BookMeta;

/** Material, ItemStack, and the string one inventory slot crosses as. */
final class Items {
    private Items() {}

    static void check() {
        materials();
        stacks();
        slots();
        potionConstructors();
        skullMeta();
        compatibilityAliases();
    }

    private static void potionConstructors() {
        org.bukkit.potion.PotionEffect effect = new org.bukkit.potion.PotionEffect(
            org.bukkit.potion.PotionEffectType.SPEED, 100, 1, true);
        Checks.expect(effect.isAmbient() && effect.hasParticles() && effect.hasIcon(),
            "four-argument potion effect keeps ambient defaults");
    }

    private static void skullMeta() {
        org.bukkit.inventory.meta.SkullMeta meta = new org.bukkit.inventory.meta.SimpleSkullMeta();
        Checks.expect(!meta.hasOwner(), "empty skull has no owner");
        meta.setOwner("example");
        Checks.expect(meta.hasOwner(), "named skull reports an owner");
    }

    private static void compatibilityAliases() {
        Checks.expect(org.bukkit.attribute.Attribute.JUMP_STRENGTH
                == org.bukkit.attribute.Attribute.GENERIC_JUMP_STRENGTH
            && org.bukkit.attribute.Attribute.STEP_HEIGHT
                == org.bukkit.attribute.Attribute.GENERIC_STEP_HEIGHT
            && org.bukkit.attribute.Attribute.SNEAKING_SPEED
                == org.bukkit.attribute.Attribute.PLAYER_SNEAKING_SPEED
            && org.bukkit.attribute.Attribute.MOVEMENT_SPEED
                == org.bukkit.attribute.Attribute.GENERIC_MOVEMENT_SPEED,
            "modern attribute names alias the existing Bukkit constants");
        Checks.expect(org.bukkit.potion.PotionEffectType.JUMP_BOOST
            == org.bukkit.potion.PotionEffectType.JUMP,
            "jump boost keeps the modern and legacy potion names identical");
    }

    private static void materials() {
        // The enum is generated from the same registry the server is built
        // from, so a name Foton knows is a name a plugin can reach.
        Checks.expect(Material.DIAMOND_SWORD != null, "a common item exists");
        Checks.expect(Material.AIR.isAir(), "air is air");
        Checks.expect(Material.STONE.isBlock() && Material.STONE.isItem(),
            "stone is both a block and an item");

        // A wall sign is a block with no item. A plugin that put one in an
        // inventory would have made a stack of nothing, and isItem is how it
        // finds out.
        Checks.expect(Material.OAK_WALL_SIGN.isBlock(), "a wall sign is a block");
        Checks.expect(!Material.OAK_WALL_SIGN.isItem(), "a wall sign is not an item");

        Checks.same(Material.DIAMOND_SWORD.getMaxStackSize(), 1, "a sword does not stack");
        Checks.same(Material.STONE.getMaxStackSize(), 64, "stone stacks to 64");

        // Three spellings, because that is what a config file contains.
        Checks.same(Material.matchMaterial("DIAMOND_SWORD"), Material.DIAMOND_SWORD, "by name");
        Checks.same(Material.matchMaterial("diamond_sword"), Material.DIAMOND_SWORD, "by key");
        Checks.same(Material.matchMaterial("minecraft:diamond_sword"), Material.DIAMOND_SWORD,
            "by namespaced key");
        Checks.same(Material.matchMaterial("not_a_thing"), null,
            "an unknown name answers null rather than throwing");
        Checks.same(Material.DIAMOND_SWORD.getKey().toString(), "minecraft:diamond_sword",
            "the key round-trips");
    }

    private static void stacks() {
        ItemStack sword = new ItemStack(Material.DIAMOND_SWORD);
        Checks.same(sword.getAmount(), 1, "a stack defaults to one");

        ItemStack stone = new ItemStack(Material.STONE, 32);
        Checks.same(stone.getAmount(), 32, "a stack keeps its count");

        // The meta comes back as a copy, which is Bukkit's trap: a plugin
        // mutates what it got and must call setItemMeta or nothing happens.
        // Behaving differently would make plugins written against it wrong.
        ItemMeta meta = sword.getItemMeta();
        Checks.expect(!meta.hasEnchant(org.bukkit.enchantments.Enchantment.SHARPNESS),
            "an empty metadata enchantment map reports no enchantment");
        meta.addEnchant(org.bukkit.enchantments.Enchantment.SHARPNESS, 2, true);
        Checks.expect(meta.hasEnchant(org.bukkit.enchantments.Enchantment.SHARPNESS),
            "hasEnchant reads the metadata's actual enchantment map");
        meta.removeEnchant(org.bukkit.enchantments.Enchantment.SHARPNESS);
        meta.setDisplayName("Excalibur");
        Checks.expect(!sword.getItemMeta().hasDisplayName(),
            "mutating the returned meta must not reach the item");
        sword.setItemMeta(meta);
        Checks.same(sword.getItemMeta().getDisplayName(), "Excalibur",
            "setItemMeta is what makes it stick");

        meta.setLore(List.of("one", "two"));
        sword.setItemMeta(meta);
        Checks.same(sword.getItemMeta().getLore(), List.of("one", "two"), "lore sticks");

        // Setting the type to air empties the stack, which is what a plugin
        // clearing a slot relies on.
        ItemStack cleared = new ItemStack(Material.STONE, 32);
        cleared.setType(Material.AIR);
        Checks.same(cleared.getAmount(), 0, "becoming air empties the stack");

        Checks.expect(new ItemStack(Material.STONE, 1).isSimilar(new ItemStack(Material.STONE, 64)),
            "isSimilar ignores the count");
        Checks.expect(!new ItemStack(Material.STONE, 1).equals(new ItemStack(Material.STONE, 64)),
            "equals does not");
        ItemStack damaged = new ItemStack(Material.DIAMOND_SWORD);
        ItemStack otherDamage = damaged.clone();
        otherDamage.setDurability((short) 1);
        Checks.expect(!damaged.equals(otherDamage),
            "different durability must make item stacks different");
        Checks.expect(!damaged.isSimilar(otherDamage),
            "isSimilar must include durability while ignoring only amount");
        Checks.expect(!sword.isSimilar(new ItemStack(Material.DIAMOND_SWORD)),
            "a named sword is not a plain one");

        ItemMeta flagged = new ItemStack(Material.STONE).getItemMeta();
        flagged.addItemFlags(org.bukkit.inventory.ItemFlag.HIDE_ATTRIBUTES);
        ItemMeta flaggedCopy = flagged.clone();
        Checks.expect(flagged.equals(flaggedCopy)
            && flagged.hashCode() == flaggedCopy.hashCode(),
            "item metadata equality must include cloned flags");
        ItemMeta unflagged = new ItemStack(Material.STONE).getItemMeta();
        Checks.expect(!flagged.equals(unflagged),
            "different item flags must make metadata different");

        BookMeta book = (BookMeta) new ItemStack(Material.WRITTEN_BOOK).getItemMeta();
        Checks.expect(book.setTitle("A precise title"), "a short book title should fit");
        book.setAuthor("Ada");
        book.addPage("one", "two");
        Checks.same(book.getTitle(), "A precise title", "a book keeps its title");
        Checks.same(book.getAuthor(), "Ada", "a book keeps its author");
        Checks.same(book.getPage(2), "two", "book pages are numbered from one");
        Checks.expect(!book.setTitle("x".repeat(33)),
            "a title longer than Bukkit's limit should be refused");
        BookMeta bookCopy = book.clone();
        bookCopy.setPage(1, "changed");
        Checks.same(book.getPage(1), "one", "a cloned book has its own page list");
        Checks.expect(!new ItemStack(Material.STONE).setItemMeta(book),
            "book metadata should not attach to a stone");
    }

    /** One slot crosses JNI as a string, so the string has to survive. */
    private static void slots() {
        ItemStack stone = new ItemStack(Material.STONE, 32);
        Checks.same(foton.FotonInventory.encode(stone), "minecraft:stone 32", "encoded");
        ItemStack named = new ItemStack(Material.STONE);
        org.bukkit.inventory.meta.ItemMeta namedMeta = named.getItemMeta();
        namedMeta.setDisplayName("named stone");
        named.setItemMeta(namedMeta);
        ItemStack namedRead = foton.FotonInventory.decode(foton.FotonInventory.encode(named));
        Checks.same(namedRead.getItemMeta().getDisplayName(), "named stone", "slot metadata name survives");
        ItemStack read = foton.FotonInventory.decode("minecraft:stone 32");
        Checks.same(read.getType(), Material.STONE, "decoded type");
        ItemStack enchanted = new ItemStack(Material.DIAMOND_SWORD);
        org.bukkit.inventory.meta.ItemMeta enchantedMeta = enchanted.getItemMeta();
        enchantedMeta.addEnchant(org.bukkit.enchantments.Enchantment.SHARPNESS, 3, true);
        enchanted.setItemMeta(enchantedMeta);
        ItemStack enchantedRead = foton.FotonInventory.decode(foton.FotonInventory.encode(enchanted));
        Checks.expect(enchantedRead.getItemMeta().getEnchantLevel(
            org.bukkit.enchantments.Enchantment.SHARPNESS) == 3,
            "slot enchantment survives");
        enchanted.addUnsafeEnchantment(org.bukkit.enchantments.Enchantment.LOOTING, 4);
        Checks.same(enchanted.getEnchantmentLevel(org.bukkit.enchantments.Enchantment.LOOTING), 4,
            "direct stack enchantment survives");
        Checks.same(enchanted.getEnchantments().get(org.bukkit.enchantments.Enchantment.LOOTING), 4,
            "stack exposes enchantments");
        Checks.same(enchanted.removeEnchantment(org.bukkit.enchantments.Enchantment.LOOTING), 4,
            "removing an enchantment returns its old level");
        Checks.same(enchanted.getEnchantmentLevel(org.bukkit.enchantments.Enchantment.LOOTING), 0,
            "removed enchantment is absent");
        boolean rejected = false;
        try { enchanted.addEnchantment(org.bukkit.enchantments.Enchantment.LOOTING, 4); }
        catch (IllegalArgumentException expected) { rejected = true; }
        Checks.expect(rejected, "safe enchantment rejects levels above its maximum");
        ItemStack modeled = new ItemStack(Material.STONE);
        org.bukkit.inventory.meta.ItemMeta modeledMeta = modeled.getItemMeta();
        modeledMeta.setCustomModelData(42);
        modeled.setItemMeta(modeledMeta);
        ItemStack modeledRead = foton.FotonInventory.decode(foton.FotonInventory.encode(modeled));
        Checks.expect(modeledRead.getItemMeta().hasCustomModelData()
            && modeledRead.getItemMeta().getCustomModelData() == 42,
            "slot custom model data survives");
        ItemStack styled = new ItemStack(Material.STONE);
        org.bukkit.inventory.meta.ItemMeta styledMeta = styled.getItemMeta();
        styledMeta.setItemModel(org.bukkit.NamespacedKey.minecraft("ruby"));
        styledMeta.setTooltipStyle(org.bukkit.NamespacedKey.minecraft("rare"));
        styledMeta.setHideTooltip(true);
        styled.setItemMeta(styledMeta);
        org.bukkit.inventory.meta.ItemMeta styledRead = foton.FotonInventory.decode(foton.FotonInventory.encode(styled)).getItemMeta();
        Checks.expect(styledRead.getItemModel().equals(org.bukkit.NamespacedKey.minecraft("ruby"))
            && styledRead.getTooltipStyle().equals(org.bukkit.NamespacedKey.minecraft("rare"))
            && styledRead.isHideTooltip(),
            "slot item model and tooltip components survive");
        ItemStack persistent = new ItemStack(Material.PAPER);
        persistent.setOpaqueNbt("{user:{untouched:1b}}");
        org.bukkit.NamespacedKey actionKey = new org.bukkit.NamespacedKey("zeldaciv", "action");
        org.bukkit.NamespacedKey markerKey = new org.bukkit.NamespacedKey("zeldaciv", "marker");
        org.bukkit.NamespacedKey amountKey = new org.bukkit.NamespacedKey("zeldaciv", "amount");
        org.bukkit.inventory.meta.ItemMeta persistentMeta = persistent.getItemMeta();
        persistentMeta.getPersistentDataContainer().set(actionKey,
            org.bukkit.persistence.PersistentDataType.STRING, "télé\0port🚀\u001d\u001e\u001f");
        persistentMeta.getPersistentDataContainer().set(markerKey,
            org.bukkit.persistence.PersistentDataType.BYTE, (byte) 1);
        persistentMeta.getPersistentDataContainer().set(amountKey,
            org.bukkit.persistence.PersistentDataType.INTEGER, 125);
        persistent.setItemMeta(persistentMeta);
        ItemStack persistentRead = foton.FotonInventory.decode(foton.FotonInventory.encode(persistent));
        org.bukkit.persistence.PersistentDataContainer persistentData =
            persistentRead.getItemMeta().getPersistentDataContainer();
        Checks.same(persistentData.get(actionKey, org.bukkit.persistence.PersistentDataType.STRING),
            "télé\0port🚀\u001d\u001e\u001f", "slot string persistent data survives");
        Checks.same(persistentData.get(markerKey, org.bukkit.persistence.PersistentDataType.BYTE),
            (byte) 1, "slot byte persistent data survives");
        Checks.same(persistentData.get(amountKey, org.bukkit.persistence.PersistentDataType.INTEGER),
            125, "slot integer persistent data survives");
        Checks.same(persistentData.get(amountKey, org.bukkit.persistence.PersistentDataType.STRING),
            null, "persistent data keeps its primitive type");
        Checks.same(persistentRead.getOpaqueNbt(), "{user:{untouched:1b}}",
            "persistent data does not overwrite opaque item NBT");
        Checks.expect(persistent.getItemMeta().equals(persistent.getItemMeta().clone()),
            "cloned item metadata keeps equal persistent data");
        ItemStack invalidUtf8 = foton.FotonInventory.decode(
            "minecraft:paper 1\u001dpdcstrhex=7a656c64616369763a616374696f6e:ff");
        Checks.same(invalidUtf8.getItemMeta().getPersistentDataContainer().get(actionKey,
            org.bukkit.persistence.PersistentDataType.STRING), null,
            "malformed UTF-8 persistent data is rejected rather than replaced");
        String actionKeyHex = "7a656c64616369763a616374696f6e";
        ItemStack nativePersistent = foton.FotonInventory.decode(
            "minecraft:paper 1\u001dpdcrawhex=0a00\u001dpdcstrhex=" + actionKeyHex + ":73616d65");
        ItemStack javaPersistent = new ItemStack(Material.PAPER);
        org.bukkit.inventory.meta.ItemMeta javaPersistentMeta = javaPersistent.getItemMeta();
        javaPersistentMeta.getPersistentDataContainer().set(actionKey,
            org.bukkit.persistence.PersistentDataType.STRING, "same");
        javaPersistent.setItemMeta(javaPersistentMeta);
        Checks.expect(nativePersistent.getItemMeta().equals(javaPersistent.getItemMeta()),
            "native passthrough does not change Bukkit metadata equality");
        ItemStack unknownNativeOne = foton.FotonInventory.decode(
            "minecraft:paper 1\u001dpdcrawhex=0a00\u001dpdcidentity=" + "00".repeat(32));
        ItemStack unknownNativeTwo = foton.FotonInventory.decode(
            "minecraft:paper 1\u001dpdcrawhex=0a00\u001dpdcidentity=" + "01".repeat(32));
        Checks.expect(!unknownNativeOne.getItemMeta().equals(unknownNativeTwo.getItemMeta())
            && !unknownNativeOne.isSimilar(unknownNativeTwo),
            "different unknown native item data changes Bukkit metadata identity");
        Checks.expect(foton.FotonInventory.encode(unknownNativeOne)
            .contains("pdcidentity=" + "00".repeat(32)),
            "unknown native item identity survives another Java bridge pass");
        org.bukkit.inventory.meta.ItemMeta removedPersistentMeta = nativePersistent.getItemMeta();
        removedPersistentMeta.getPersistentDataContainer().remove(actionKey);
        nativePersistent.setItemMeta(removedPersistentMeta);
        String removedPersistent = foton.FotonInventory.encode(nativePersistent);
        Checks.expect(removedPersistent.contains("pdcremove=" + actionKeyHex)
            && !removedPersistent.contains("pdcstrhex=" + actionKeyHex),
            "removing native persistent data emits one explicit tombstone");

        ItemStack malformedUtf16 = new ItemStack(Material.PAPER);
        org.bukkit.inventory.meta.ItemMeta malformedUtf16Meta = malformedUtf16.getItemMeta();
        malformedUtf16Meta.getPersistentDataContainer().set(actionKey,
            org.bukkit.persistence.PersistentDataType.STRING, "\ud800");
        malformedUtf16.setItemMeta(malformedUtf16Meta);
        boolean malformedUtf16Rejected = false;
        try { foton.FotonInventory.encode(malformedUtf16); }
        catch (IllegalArgumentException expected) { malformedUtf16Rejected = true; }
        Checks.expect(malformedUtf16Rejected,
            "isolated UTF-16 surrogates are rejected by item persistent data");

        ItemStack boundaryPersistent = new ItemStack(Material.PAPER);
        org.bukkit.inventory.meta.ItemMeta boundaryMeta = boundaryPersistent.getItemMeta();
        boundaryMeta.getPersistentDataContainer().set(actionKey,
            org.bukkit.persistence.PersistentDataType.STRING, "\0".repeat(32_767) + "a");
        boundaryPersistent.setItemMeta(boundaryMeta);
        Checks.same(foton.FotonInventory.decode(foton.FotonInventory.encode(boundaryPersistent))
            .getItemMeta().getPersistentDataContainer().get(actionKey,
                org.bukkit.persistence.PersistentDataType.STRING).length(),
            32_768, "65,535 modified UTF-8 bytes fit in item persistent data");
        boundaryMeta = boundaryPersistent.getItemMeta();
        boundaryMeta.getPersistentDataContainer().set(actionKey,
            org.bukkit.persistence.PersistentDataType.STRING, "\0".repeat(32_768));
        boundaryPersistent.setItemMeta(boundaryMeta);
        boolean oversizedPersistentRejected = false;
        try { foton.FotonInventory.encode(boundaryPersistent); }
        catch (IllegalArgumentException expected) { oversizedPersistentRejected = true; }
        Checks.expect(oversizedPersistentRejected,
            "65,536 modified UTF-8 bytes are rejected before NBT encoding");
        ItemStack potion = new ItemStack(Material.POTION);
        org.bukkit.inventory.meta.ItemMeta potionMeta = potion.getItemMeta();
        potionMeta.setDisplayName("potion");
        potionMeta.setLore(java.util.List.of("line"));
        potion.setItemMeta(potionMeta);
        org.bukkit.inventory.meta.PotionMeta potionData = (org.bukkit.inventory.meta.PotionMeta) potion.getItemMeta();
        potionData.addCustomEffect(new org.bukkit.potion.PotionEffect(org.bukkit.potion.PotionEffectType.SPEED, 20, 1), true);
        potion.setItemMeta(potionData);
        org.bukkit.inventory.meta.PotionMeta potionRead = (org.bukkit.inventory.meta.PotionMeta)
            foton.FotonInventory.decode(foton.FotonInventory.encode(potion)).getItemMeta();
        Checks.expect(potionRead.getCustomEffects().size() == 1
            && potionRead.getDisplayName().equals("potion")
            && potionRead.getLore().equals(java.util.List.of("line")),
            "slot metadata fields coexist with potion effects");
        Checks.same(read.getAmount(), 32, "decoded amount");

        // Empty and unreadable are different answers. An empty slot is the
        // empty string; anything that cannot be read is null, so a plugin
        // never reads a slot it could not see as an empty one.
        Checks.same(foton.FotonInventory.encode(null), "", "nothing encodes to nothing");
        Checks.same(foton.FotonInventory.encode(new ItemStack(Material.AIR)), "",
            "air encodes to nothing");
        Checks.same(foton.FotonInventory.decode(""), null, "an empty slot decodes to nothing");
        Checks.same(foton.FotonInventory.decode(null), null, "an unreadable slot is nothing");
        Checks.same(foton.FotonInventory.decode("minecraft:not_a_thing 1"), null,
            "an unknown item is nothing rather than a guess");
        Checks.same(foton.FotonInventory.decode("minecraft:stone 0"), null,
            "a count of zero is nothing");

        org.bukkit.block.ShulkerBox box = new foton.FotonShulkerBox();
        Checks.same(box.getBlockData().getMaterial(), Material.SHULKER_BOX, "shulker snapshot has block data");
        org.bukkit.inventory.Inventory shulker = box.getInventory();
        Checks.expect(shulker.getHolder() == box, "shulker inventory exposes its holder");
        shulker.addItem(new ItemStack(Material.STONE, 70));
        Checks.same(shulker.getItem(0).getAmount(), 64, "shulker inventory respects stack limits");
        Checks.same(shulker.getItem(1).getAmount(), 6, "shulker inventory splits oversized stacks");
        org.bukkit.inventory.Inventory snapshot = box.getSnapshotInventory();
        snapshot.clear(0);
        Checks.expect(box.getInventory().getItem(0) != null,
            "shulker snapshot is independent");
    }
}
