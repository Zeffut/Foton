package org.bukkit.ban;

import com.destroystokyo.paper.profile.PlayerProfile;
import java.util.Date;
import org.bukkit.BanEntry;
import org.bukkit.BanList;

/** The server's player bans, by profile. */
public interface ProfileBanList extends BanList<PlayerProfile> {
    <E extends BanEntry<? super PlayerProfile>> E addBan(org.bukkit.profile.PlayerProfile target, String reason, Date expires, String source);
    <E extends BanEntry<? super PlayerProfile>> E getBanEntry(org.bukkit.profile.PlayerProfile target);
    boolean isBanned(org.bukkit.profile.PlayerProfile target);
    void pardon(org.bukkit.profile.PlayerProfile target);
    <E extends BanEntry<? super PlayerProfile>> E addBan(org.bukkit.profile.PlayerProfile target, String reason, java.time.Instant expires, String source);
    <E extends BanEntry<? super PlayerProfile>> E addBan(org.bukkit.profile.PlayerProfile target, String reason, java.time.Duration duration, String source);
}
