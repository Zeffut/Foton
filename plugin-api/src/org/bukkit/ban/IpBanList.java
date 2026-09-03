package org.bukkit.ban;

import org.bukkit.BanEntry;

public interface IpBanList {
    BanEntry<?> getBanEntry(Object target);
}
