package foton;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.Date;
import java.util.List;
import org.bukkit.advancement.Advancement;
import org.bukkit.advancement.AdvancementProgress;

/** An online player's progress on one advancement, read from the player
 * each time it is asked. Awarding a criterion is main-thread work, as it is
 * in Paper: it may finish the advancement and hand out its rewards. */
final class FotonAdvancementProgress implements AdvancementProgress {
    private final String player;
    private final Advancement advancement;

    FotonAdvancementProgress(String player, Advancement advancement) {
        this.player = player;
        this.advancement = advancement;
    }

    private String[] read() {
        String[] progress = Native.playerAdvancementProgress(player, advancement.getKey().toString());
        return progress == null || progress.length == 0 ? new String[] {"0"} : progress;
    }

    private List<String> criteria(boolean awarded) {
        String[] progress = read();
        List<String> out = new ArrayList<>();
        for (int index = 1; index < progress.length; index++) {
            int separator = progress[index].indexOf('\u001f');
            if ((separator >= 0) == awarded) {
                out.add(separator >= 0 ? progress[index].substring(0, separator) : progress[index]);
            }
        }
        return Collections.unmodifiableList(out);
    }

    @Override public Advancement getAdvancement() { return advancement; }
    @Override public boolean isDone() { return "1".equals(read()[0]); }

    @Override
    public boolean awardCriteria(String criteria) {
        return criteria != null && Native.playerAdvancementCriterion(player, advancement.getKey().toString(), criteria, true);
    }

    @Override
    public boolean revokeCriteria(String criteria) {
        return criteria != null && Native.playerAdvancementCriterion(player, advancement.getKey().toString(), criteria, false);
    }

    @Override
    public Date getDateAwarded(String criteria) {
        if (criteria == null) return null;
        String[] progress = read();
        for (int index = 1; index < progress.length; index++) {
            String prefix = criteria + '\u001f';
            if (progress[index].startsWith(prefix)) {
                return new Date(Long.parseLong(progress[index].substring(prefix.length())));
            }
        }
        return null;
    }

    @Override public Collection<String> getRemainingCriteria() { return criteria(false); }
    @Override public Collection<String> getAwardedCriteria() { return criteria(true); }
}
