package foton;
import java.util.*;
import org.bukkit.command.Command;
import org.bukkit.help.*;
public final class FotonHelpMap implements HelpMap {
    private final Map<String,HelpTopic> topics = new LinkedHashMap<>();
    private final Map<Class<? extends Command>,HelpTopicFactory<?>> factories = new LinkedHashMap<>();
    public FotonHelpMap() { refresh(); }
    public void refresh() {
        for (Command command : CommandMap.knownCommands().values()) {
            if (topics.containsKey("/"+command.getName())) continue;
            HelpTopic topic = create(command);
            if (topic != null) topics.put(topic.getName(), topic);
        }
    }
    private HelpTopic create(Command command) {
        HelpTopicFactory<?> factory = null;
        for (Map.Entry<Class<? extends Command>,HelpTopicFactory<?>> e : factories.entrySet()) if (e.getKey().isInstance(command)) { factory=e.getValue(); break; }
        if (factory != null) return createWith(factory, command);
        return new HelpTopic() {
            public String getName() { return "/"+command.getName(); }
            public String getShortText() { return command.getDescription(); }
            public String getFullText(org.bukkit.command.CommandSender sender) { return command.getUsage(); }
        };
    }
    @SuppressWarnings("unchecked") private static HelpTopic createWith(HelpTopicFactory<?> f, Command c) { return ((HelpTopicFactory<Command>)f).createTopic(c); }
    public HelpTopic getHelpTopic(String name) { refresh(); return topics.get(name); }
    public Collection<HelpTopic> getHelpTopics() { refresh(); return Collections.unmodifiableCollection(new ArrayList<>(topics.values())); }
    public <T extends Command> void registerHelpTopicFactory(Class<T> type, HelpTopicFactory<T> factory) { if (type != null && factory != null) factories.put(type, factory); topics.clear(); }
}
