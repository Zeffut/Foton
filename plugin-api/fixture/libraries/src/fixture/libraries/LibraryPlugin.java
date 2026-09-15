package fixture.libraries;

public final class LibraryPlugin extends org.bukkit.plugin.java.JavaPlugin {
    @Override
    public void onLoad() {
        System.setProperty("foton.fixture.libraries.descriptor", DescriptorLibraryApi.value());
        System.setProperty("foton.fixture.libraries.paper", PaperLibraryApi.value());
    }
}
