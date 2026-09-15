package example;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.plugin.java.JavaPlugin;

public final class PaperPlugin extends JavaPlugin {
    public static final List<String> steps = new ArrayList<>();

    public PaperPlugin() {
        steps.add("constructor:default");
    }

    public PaperPlugin(String sentinel) {
        steps.add("constructor:" + sentinel);
    }

    @Override
    public void onLoad() {
        steps.add("onLoad");
    }

    @Override
    public void onEnable() {
        steps.add("onEnable");
    }
}
