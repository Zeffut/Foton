import java.io.StringReader;
import org.bukkit.plugin.PluginDescriptionFile;

/** Runs unchanged against the Foton API and the pinned official Paper API. */
public final class DescriptorParity {
    public static void main(String[] args) throws Exception {
        for (String fields : new String[] {
                "provides: [Alias]\nload: STARTUP\nlibraries: [org.example:library:1.0]\n",
                "libraries: [org.example:library:1.0]\npaper-skip-libraries: true\n",
                "permissions:\n  fixture.default: {}\n",
                "default-permission: false\npermissions:\n  fixture.default: {}\n",
                "contributors: [Ada]\nauthors: [Alan]\nauthor: Grace\nprefix: Prefix\nwebsite: https://example.org\n"
        }) {
            PluginDescriptionFile descriptor = new PluginDescriptionFile(new StringReader(
                "name: Fixture\nversion: 1\nmain: fixture.Main\n" + fields));
            System.out.println(descriptor.getProvides() + "|" + descriptor.getLoad() + "|"
                + descriptor.getLibraries() + "|" + descriptor.getPermissionDefault().name() + "|"
                + descriptor.getPermissions().stream().map(p -> p.getName() + "=" + p.getDefault().name()).toList()
                + "|" + descriptor.getContributors() + "|" + descriptor.getAuthors() + "|"
                + descriptor.getPrefix() + "|" + descriptor.getWebsite());
        }
    }
}
