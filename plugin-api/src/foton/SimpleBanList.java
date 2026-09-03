package foton;

import java.util.Collections;
import java.util.Date;
import java.util.LinkedHashSet;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import org.bukkit.BanEntry;
import org.bukkit.BanList;

/** Thread-safe runtime ban list shared by Bukkit callers and admission. */
final class SimpleBanList<T> implements BanList<T> {
    private final Map<T, Entry> entries = new ConcurrentHashMap<>();

    @Override public BanEntry<T> getBanEntry(T target) {
        Entry entry = target == null ? null : entries.get(target);
        if (entry != null && entry.isExpired()) { entries.remove(target, entry); return null; }
        return entry;
    }

    @Override public BanEntry<T> addBan(T target, String reason, Date expiration, String source) {
        if (target == null) return null;
        Entry entry = new Entry(target, reason, expiration, source);
        entries.put(target, entry);
        return entry;
    }

    @Override public boolean isBanned(T target) { return getBanEntry(target) != null; }
    boolean isBannedIgnoreCase(String target) {
        if (target == null) return false;
        for (T candidate : entries.keySet()) {
            if (candidate instanceof String value && value.equalsIgnoreCase(target) && isBanned(candidate)) return true;
        }
        return false;
    }
    @Override public void pardon(T target) { if (target != null) entries.remove(target); }

    @Override public Set<BanEntry<T>> getBanEntries() {
        Set<BanEntry<T>> result = new LinkedHashSet<>();
        for (T target : entries.keySet()) { BanEntry<T> entry = getBanEntry(target); if (entry != null) result.add(entry); }
        return Collections.unmodifiableSet(result);
    }

    private final class Entry implements BanEntry<T> {
        private final T target; private volatile Date created = new Date();
        private volatile Date expiration; private volatile String reason; private volatile String source;
        Entry(T target, String reason, Date expiration, String source) {
            this.target = target; this.reason = reason; this.expiration = expiration; this.source = source;
        }
        boolean isExpired() { return expiration != null && expiration.getTime() <= System.currentTimeMillis(); }
        @Override public T getTarget() { return target; }
        @Override public Date getCreated() { return created == null ? null : new Date(created.getTime()); }
        @Override public void setCreated(Date value) { created = value == null ? null : new Date(value.getTime()); }
        @Override public Date getExpiration() { return expiration == null ? null : new Date(expiration.getTime()); }
        @Override public void setExpiration(Date value) { expiration = value == null ? null : new Date(value.getTime()); }
        @Override public String getReason() { return reason; }
        @Override public void setReason(String value) { reason = value; }
        @Override public String getSource() { return source; }
        @Override public void setSource(String value) { source = value; }
        @Override public void save() { entries.put(target, this); }
        @Override public void remove() { entries.remove(target, this); }
    }
}
