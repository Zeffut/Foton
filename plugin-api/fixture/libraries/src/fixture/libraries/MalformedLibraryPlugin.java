package fixture.libraries;

public final class MalformedLibraryPlugin extends org.bukkit.plugin.java.JavaPlugin {
    @Override
    public void onLoad() {
        String loaded = System.getProperty("foton.fixture.libraries.malformed", "");
        System.setProperty("foton.fixture.libraries.malformed",
            loaded.isEmpty() ? getName() : loaded + "," + getName());
    }
}
