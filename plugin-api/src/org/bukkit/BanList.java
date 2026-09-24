package org.bukkit;

import java.util.Date;
import java.util.Set;

/** Access to bans maintained by the running server. */
public interface BanList<T> {
    enum Type { NAME, IP, PROFILE }
    BanEntry<T> getBanEntry(T target);
    @SuppressWarnings("unchecked")
    default BanEntry<T> getBanEntry(String target) { return getBanEntry((T) target); }
    BanEntry<T> addBan(T target, String reason, Date expiration, String source);
    @SuppressWarnings("unchecked")
    default BanEntry<T> addBan(String target, String reason, Date expiration, String source) {
        return addBan((T) target, reason, expiration, source);
    }
    /** Bans until an instant; null bans for good. */
    default BanEntry<T> addBan(T target, String reason, java.time.Instant expires, String source) {
        return addBan(target, reason, expires == null ? null : Date.from(expires), source);
    }
    /** Bans for a while from now; null bans for good. */
    default BanEntry<T> addBan(T target, String reason, java.time.Duration duration, String source) {
        return addBan(target, reason,
            duration == null ? null : Date.from(java.time.Instant.now().plus(duration)), source);
    }
    default void addBan(BanEntry<T> entry) {
        if (entry != null) addBan(entry.getTarget(), entry.getReason(), entry.getExpiration(), entry.getSource());
    }
    boolean isBanned(T target);
    @SuppressWarnings("unchecked")
    default boolean isBanned(String target) { return isBanned((T) target); }
    void pardon(T target);
    @SuppressWarnings("unchecked")
    default void pardon(String target) { pardon((T) target); }
    Set<BanEntry<T>> getBanEntries();
}
