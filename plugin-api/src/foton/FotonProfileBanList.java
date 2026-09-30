package foton;

import com.destroystokyo.paper.profile.PlayerProfile;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.text.ParseException;
import java.text.SimpleDateFormat;
import java.util.Collections;
import java.util.Date;
import java.util.LinkedHashSet;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.logging.Level;
import java.util.logging.Logger;
import org.bukkit.BanEntry;
import org.bukkit.ban.ProfileBanList;

/** The server's player bans, kept in {@code banned-players.json} in
 * vanilla's own format so that they outlive a restart and read the same way
 * vanilla reads them.
 *
 * A ban is keyed by the profile's UUID, as vanilla's is. A profile with only
 * a name -- what the deprecated name-based Bukkit calls produce -- is kept by
 * that name, matched without regard to case, and written without a UUID. */
final class FotonProfileBanList implements ProfileBanList {
    /** Vanilla's `BanListEntry.DATE_FORMAT`. */
    private static final String DATE_PATTERN = "yyyy-MM-dd HH:mm:ss Z";
    /** Vanilla's `PlayerList.BAN_DATE_FORMAT`, which the kick message uses. */
    private static final String KICK_DATE_PATTERN = "yyyy-MM-dd 'at' HH:mm:ss z";
    private static final String FOREVER = "forever";
    private static final String UNKNOWN = "(Unknown)";
    private static final Logger LOG = Logger.getLogger("Foton");

    private final File file;
    private final Map<String, Entry> entries = new ConcurrentHashMap<>();

    FotonProfileBanList(File file) {
        this.file = file;
        load();
    }

    private static String key(UUID id, String name) {
        if (id != null) return id.toString();
        return name == null ? null : "name:" + name.toLowerCase(Locale.ROOT);
    }

    private static String key(org.bukkit.profile.PlayerProfile profile) {
        return profile == null ? null : key(profile.getUniqueId(), profile.getName());
    }

    /** The ban that applies to a player joining as `id` and `name`, if any. */
    Entry banOf(UUID id, String name) {
        Entry byId = live(key(id, null));
        return byId != null ? byId : live(key(null, name));
    }

    /** What a banned player, or a player from a banned address, is told, in
     * vanilla's English: `multiplayer.disconnect.banned.reason` or
     * `banned_ip.reason`, then the expiration line when the ban ends. */
    static String kickMessage(BanEntry<?> entry, boolean address) {
        String reason = entry.getReason() == null ? "Banned by an operator." : entry.getReason();
        StringBuilder message = new StringBuilder(address
            ? "Your IP address is banned from this server.\nReason: "
            : "You are banned from this server.\nReason: ").append(reason);
        if (entry.getExpiration() != null) {
            message.append("\nYour ban will be removed on ")
                .append(new SimpleDateFormat(KICK_DATE_PATTERN, Locale.ROOT).format(entry.getExpiration()));
        }
        return message.toString();
    }

    private Entry live(String key) {
        if (key == null) return null;
        Entry entry = entries.get(key);
        if (entry == null || !entry.isExpired()) return entry;
        if (entries.remove(key, entry)) save();
        return null;
    }

    // ---- BanList<PlayerProfile> -------------------------------------------

    @Override public BanEntry<PlayerProfile> getBanEntry(PlayerProfile target) { return live(key(target)); }

    @Override public BanEntry<PlayerProfile> getBanEntry(String target) { return live(key(null, target)); }

    @Override
    public BanEntry<PlayerProfile> addBan(PlayerProfile target, String reason, Date expires, String source) {
        String key = key(target);
        if (key == null) return null;
        Entry entry = new Entry(new FotonPlayerProfile(target.getUniqueId(), target.getName()), new Date(),
            source == null ? UNKNOWN : source, expires, reason);
        entries.put(key, entry);
        save();
        return entry;
    }

    @Override
    public BanEntry<PlayerProfile> addBan(String target, String reason, Date expires, String source) {
        return addBan((PlayerProfile) new FotonPlayerProfile(null, target), reason, expires, source);
    }

    @Override public boolean isBanned(PlayerProfile target) { return getBanEntry(target) != null; }
    @Override public boolean isBanned(String target) { return live(key(null, target)) != null; }

    @Override public void pardon(PlayerProfile target) { remove(key(target)); }
    @Override public void pardon(String target) { remove(key(null, target)); }

    @Override
    public Set<BanEntry<PlayerProfile>> getBanEntries() {
        Set<BanEntry<PlayerProfile>> result = new LinkedHashSet<>();
        for (String key : entries.keySet()) {
            Entry entry = live(key);
            if (entry != null) result.add(entry);
        }
        return Collections.unmodifiableSet(result);
    }

    // ---- ProfileBanList, for any Bukkit profile ----------------------------

    @Override
    @SuppressWarnings("unchecked")
    public <E extends BanEntry<? super PlayerProfile>> E addBan(org.bukkit.profile.PlayerProfile target, String reason,
            Date expires, String source) {
        if (target == null) return null;
        return (E) addBan(new FotonPlayerProfile(target.getUniqueId(), target.getName()), reason, expires, source);
    }

    @Override
    @SuppressWarnings("unchecked")
    public <E extends BanEntry<? super PlayerProfile>> E getBanEntry(org.bukkit.profile.PlayerProfile target) {
        return (E) live(key(target));
    }

    @Override public boolean isBanned(org.bukkit.profile.PlayerProfile target) { return live(key(target)) != null; }
    @Override public void pardon(org.bukkit.profile.PlayerProfile target) { remove(key(target)); }

    @Override
    public <E extends BanEntry<? super PlayerProfile>> E addBan(org.bukkit.profile.PlayerProfile target, String reason,
            java.time.Instant expires, String source) {
        return addBan(target, reason, expires == null ? null : Date.from(expires), source);
    }

    @Override
    public <E extends BanEntry<? super PlayerProfile>> E addBan(org.bukkit.profile.PlayerProfile target, String reason,
            java.time.Duration duration, String source) {
        return addBan(target, reason,
            duration == null ? null : Date.from(java.time.Instant.now().plus(duration)), source);
    }

    private void remove(String key) {
        if (key != null && entries.remove(key) != null) save();
    }

    // ---- banned-players.json -----------------------------------------------

    private void load() {
        if (!file.isFile()) return;
        try {
            JsonElement root = JsonParser.parseString(Files.readString(file.toPath(), StandardCharsets.UTF_8));
            if (!root.isJsonArray()) return;
            for (JsonElement element : root.getAsJsonArray()) {
                if (!element.isJsonObject()) continue;
                Entry entry = read(element.getAsJsonObject());
                String key = entry == null ? null : key(entry.target);
                if (key != null && !entry.isExpired()) entries.put(key, entry);
            }
        } catch (IOException | RuntimeException unreadable) {
            LOG.log(Level.WARNING, "Could not read " + file + "; starting with no player bans", unreadable);
        }
    }

    /** One entry of the file, read as vanilla's `UserBanListEntry` reads it:
     * an unreadable creation date is now, an unreadable expiry is never. */
    private Entry read(JsonObject json) {
        UUID id = null;
        String uuid = text(json, "uuid");
        if (uuid != null) {
            try {
                id = UUID.fromString(uuid);
            } catch (IllegalArgumentException notAUuid) {
                return null;
            }
        }
        String name = text(json, "name");
        if (id == null && name == null) return null;
        Date created = date(text(json, "created"));
        String source = text(json, "source");
        String expires = text(json, "expires");
        Date expiration = expires == null || FOREVER.equalsIgnoreCase(expires) ? null : date(expires);
        return new Entry(new FotonPlayerProfile(id, name), created == null ? new Date() : created,
            source == null ? UNKNOWN : source, expiration, text(json, "reason"));
    }

    private static String text(JsonObject json, String key) {
        JsonElement value = json.get(key);
        return value == null || !value.isJsonPrimitive() ? null : value.getAsString();
    }

    private static Date date(String text) {
        if (text == null) return null;
        try {
            return new SimpleDateFormat(DATE_PATTERN, Locale.ROOT).parse(text);
        } catch (ParseException unreadable) {
            return null;
        }
    }

    private synchronized void save() {
        JsonArray array = new JsonArray();
        for (Entry entry : entries.values()) array.add(entry.write());
        try {
            Files.writeString(file.toPath(), new GsonBuilder().setPrettyPrinting().create().toJson(array),
                StandardCharsets.UTF_8);
        } catch (IOException unwritable) {
            LOG.log(Level.WARNING, "Could not save " + file, unwritable);
        }
    }

    /** One ban. Changes to it are kept by {@link #save()}, as Bukkit says. */
    final class Entry implements BanEntry<PlayerProfile> {
        private final PlayerProfile target;
        private volatile Date created;
        private volatile String source;
        private volatile Date expiration;
        private volatile String reason;

        Entry(PlayerProfile target, Date created, String source, Date expiration, String reason) {
            this.target = target;
            this.created = created;
            this.source = source;
            this.expiration = expiration;
            this.reason = reason;
        }

        boolean isExpired() { return expiration != null && expiration.getTime() <= System.currentTimeMillis(); }

        JsonObject write() {
            JsonObject json = new JsonObject();
            if (target.getUniqueId() != null) json.addProperty("uuid", target.getUniqueId().toString());
            if (target.getName() != null) json.addProperty("name", target.getName());
            SimpleDateFormat format = new SimpleDateFormat(DATE_PATTERN, Locale.ROOT);
            json.addProperty("created", format.format(created == null ? new Date() : created));
            json.addProperty("source", source == null ? UNKNOWN : source);
            json.addProperty("expires", expiration == null ? FOREVER : format.format(expiration));
            json.addProperty("reason", reason == null ? "Banned by an operator." : reason);
            return json;
        }

        @Override public PlayerProfile getTarget() { return target; }
        @Override public Date getCreated() { return created == null ? null : new Date(created.getTime()); }
        @Override public void setCreated(Date value) { created = value == null ? null : new Date(value.getTime()); }
        @Override public Date getExpiration() { return expiration == null ? null : new Date(expiration.getTime()); }
        @Override public void setExpiration(Date value) { expiration = value == null ? null : new Date(value.getTime()); }
        @Override public String getReason() { return reason; }
        @Override public void setReason(String value) { reason = value; }
        @Override public String getSource() { return source; }
        @Override public void setSource(String value) { source = value; }
        @Override public void save() {
            String key = key(target);
            if (key == null) return;
            entries.put(key, this);
            FotonProfileBanList.this.save();
        }
        @Override public void remove() { pardon(target); }
    }
}
