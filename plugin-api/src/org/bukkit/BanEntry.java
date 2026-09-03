package org.bukkit;

import java.util.Date;

/** A server ban record. */
public interface BanEntry<T> {
    T getTarget();
    Date getCreated();
    void setCreated(Date created);
    Date getExpiration();
    void setExpiration(Date expiration);
    String getReason();
    void setReason(String reason);
    String getSource();
    void setSource(String source);
    void save();
    void remove();
}
