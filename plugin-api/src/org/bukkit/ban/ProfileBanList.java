package org.bukkit.ban;

import org.bukkit.BanEntry;
import org.bukkit.BanList;

public interface ProfileBanList extends BanList<Object> {
    @Override BanEntry<Object> getBanEntry(Object target);
}
