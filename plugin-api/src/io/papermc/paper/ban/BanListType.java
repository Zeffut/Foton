package io.papermc.paper.ban;

import org.bukkit.ban.IpBanList;
import org.bukkit.ban.ProfileBanList;

/** Which ban list {@code Server.getBanList} returns. */
public interface BanListType<T> {
    BanListType<IpBanList> IP = new BanListTypeImpl<>(IpBanList.class);
    BanListType<ProfileBanList> PROFILE = new BanListTypeImpl<>(ProfileBanList.class);

    Class<T> typeClass();
}
